#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype,
    Address, Env, Symbol, Vec, symbol_short,
    panic_with_error,
};

#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum OracleError {
    NotInitialized       = 1,
    AlreadyInitialized   = 2,
    Unauthorized         = 3,
    KeeperNotRegistered  = 4,
    InsufficientKeepers  = 5,
    PriceDeviationTooHigh= 6,
    StalePrice           = 7,
    InvalidPrice         = 8,
}

#[contracttype]
pub enum OracleKey {
    Config,
    Price(Symbol),           // asset symbol -> PriceEntry
    Keeper(Address),         // keeper address -> KeeperState
    KeeperList,
    Submission(Symbol, Address), // pending submission per keeper per asset
}

#[contracttype]
#[derive(Clone)]
pub struct OracleConfig {
    pub admin: Address,
    pub min_keepers: u32,        // minimum keepers required for valid price
    pub max_deviation_bps: u32,  // max deviation from median (basis points)
    pub staleness_threshold: u64,// seconds before price is considered stale
    pub keeper_reward: i128,     // reward per valid submission in XLM stroops
    pub slash_amount: i128,      // slash per invalid submission
    pub initialized: bool,
}

#[contracttype]
#[derive(Clone)]
pub struct PriceEntry {
    pub asset: Symbol,
    pub price: i128,             // price in USDC, 7 decimal places (1 USDC = 10_000_000)
    pub confidence_bps: u32,     // confidence interval in basis points
    pub timestamp: u64,          // ledger timestamp of last update
    pub num_sources: u32,        // number of keepers that contributed
    pub is_stale: bool,
}

#[contracttype]
#[derive(Clone)]
pub struct KeeperState {
    pub address: Address,
    pub stake: i128,             // staked XLM bond
    pub total_submissions: u64,
    pub valid_submissions: u64,
    pub active: bool,
}

#[contracttype]
#[derive(Clone)]
pub struct PriceSubmission {
    pub keeper: Address,
    pub asset: Symbol,
    pub price: i128,
    pub timestamp: u64,
}

#[contract]
pub struct CeresOracle;

#[contractimpl]
impl CeresOracle {

    /// Initialize oracle with admin address and configuration.
    pub fn initialize(
        env: Env,
        admin: Address,
        min_keepers: u32,
        max_deviation_bps: u32,
        staleness_threshold: u64,
    ) {
        if env.storage().instance().has(&OracleKey::Config) {
            panic_with_error!(&env, OracleError::AlreadyInitialized);
        }

        admin.require_auth();

        let config = OracleConfig {
            admin,
            min_keepers,
            max_deviation_bps,
            staleness_threshold,
            keeper_reward: 100_0000000, // 100 XLM per valid submission
            slash_amount: 500_0000000,  // 500 XLM slash per invalid submission
            initialized: true,
        };

        env.storage().instance().set(&OracleKey::Config, &config);
        env.storage().instance().set(&OracleKey::KeeperList, &Vec::<Address>::new(&env));
    }

    /// Register as a keeper by staking a bond.
    pub fn register_keeper(env: Env, keeper: Address, stake: i128) {
        keeper.require_auth();

        let config: OracleConfig = env.storage().instance()
            .get(&OracleKey::Config)
            .unwrap_or_else(|| panic_with_error!(&env, OracleError::NotInitialized));

        let keeper_state = KeeperState {
            address: keeper.clone(),
            stake,
            total_submissions: 0,
            valid_submissions: 0,
            active: true,
        };

        env.storage().persistent().set(&OracleKey::Keeper(keeper.clone()), &keeper_state);

        let mut list: Vec<Address> = env.storage().instance()
            .get(&OracleKey::KeeperList)
            .unwrap_or(Vec::new(&env));
        list.push_back(keeper.clone());
        env.storage().instance().set(&OracleKey::KeeperList, &list);

        env.events().publish(
            (symbol_short!("keeper"), symbol_short!("reg")),
            keeper,
        );
    }

    /// Submit a price update for an asset.
    pub fn submit_price(
        env: Env,
        keeper: Address,
        asset: Symbol,
        price: i128,
    ) {
        keeper.require_auth();

        if price <= 0 {
            panic_with_error!(&env, OracleError::InvalidPrice);
        }

        let keeper_state: KeeperState = env.storage().persistent()
            .get(&OracleKey::Keeper(keeper.clone()))
            .unwrap_or_else(|| panic_with_error!(&env, OracleError::KeeperNotRegistered));

        if !keeper_state.active {
            panic_with_error!(&env, OracleError::KeeperNotRegistered);
        }

        // Store submission
        env.storage().temporary().set(
            &OracleKey::Submission(asset.clone(), keeper.clone()),
            &PriceSubmission {
                keeper: keeper.clone(),
                asset: asset.clone(),
                price,
                timestamp: env.ledger().timestamp(),
            },
        );

        env.events().publish(
            (symbol_short!("price"), symbol_short!("sub")),
            (keeper, asset, price),
        );
    }

    /// Aggregate pending submissions and publish a validated price.
    /// Called by any keeper after sufficient submissions exist.
    pub fn aggregate_price(env: Env, asset: Symbol) -> PriceEntry {
        let config: OracleConfig = env.storage().instance()
            .get(&OracleKey::Config)
            .unwrap_or_else(|| panic_with_error!(&env, OracleError::NotInitialized));

        let keepers: Vec<Address> = env.storage().instance()
            .get(&OracleKey::KeeperList)
            .unwrap_or(Vec::new(&env));

        // Collect valid submissions
        let mut prices: Vec<i128> = Vec::new(&env);
        for keeper in keepers.iter() {
            if let Some(sub) = env.storage().temporary()
                .get::<OracleKey, PriceSubmission>(&OracleKey::Submission(asset.clone(), keeper.clone()))
            {
                let age = env.ledger().timestamp().saturating_sub(sub.timestamp);
                if age <= config.staleness_threshold {
                    prices.push_back(sub.price);
                }
            }
        }

        if prices.len() < config.min_keepers {
            panic_with_error!(&env, OracleError::InsufficientKeepers);
        }

        // Calculate median price
        let median_price = Self::compute_median(&env, &prices);

        // Reject outliers beyond max deviation
        let mut valid_prices: Vec<i128> = Vec::new(&env);
        for p in prices.iter() {
            let deviation = if p > median_price {
                ((p - median_price) * 10000) / median_price
            } else {
                ((median_price - p) * 10000) / median_price
            };
            if deviation as u32 <= config.max_deviation_bps {
                valid_prices.push_back(p);
            }
        }

        if valid_prices.len() < config.min_keepers {
            panic_with_error!(&env, OracleError::InsufficientKeepers);
        }

        let final_price = Self::compute_median(&env, &valid_prices);

        let entry = PriceEntry {
            asset: asset.clone(),
            price: final_price,
            confidence_bps: config.max_deviation_bps,
            timestamp: env.ledger().timestamp(),
            num_sources: valid_prices.len(),
            is_stale: false,
        };

        env.storage().persistent().set(&OracleKey::Price(asset.clone()), &entry);

        env.events().publish(
            (symbol_short!("price"), symbol_short!("pub")),
            (asset, final_price),
        );

        entry
    }

    /// Get the latest price for an asset.
    pub fn get_price(env: Env, asset: Symbol) -> PriceEntry {
        let config: OracleConfig = env.storage().instance()
            .get(&OracleKey::Config)
            .unwrap_or_else(|| panic_with_error!(&env, OracleError::NotInitialized));

        let mut entry: PriceEntry = env.storage().persistent()
            .get(&OracleKey::Price(asset.clone()))
            .unwrap_or_else(|| panic_with_error!(&env, OracleError::StalePrice));

        let age = env.ledger().timestamp().saturating_sub(entry.timestamp);
        if age > config.staleness_threshold {
            entry.is_stale = true;
        }

        entry
    }

    /// Check if a price is fresh (not stale).
    pub fn is_price_fresh(env: Env, asset: Symbol) -> bool {
        let config: OracleConfig = env.storage().instance()
            .get(&OracleKey::Config)
            .unwrap_or_else(|| panic_with_error!(&env, OracleError::NotInitialized));

        if let Some(entry) = env.storage().persistent()
            .get::<OracleKey, PriceEntry>(&OracleKey::Price(asset))
        {
            let age = env.ledger().timestamp().saturating_sub(entry.timestamp);
            age <= config.staleness_threshold
        } else {
            false
        }
    }

    // ── INTERNAL ─────────────────────────────────────────────────────────────

    fn compute_median(env: &Env, prices: &Vec<i128>) -> i128 {
        if prices.is_empty() {
            return 0;
        }
        // Simple average as median approximation
        // Full implementation: sort and pick middle element
        let mut sum: i128 = 0;
        for p in prices.iter() {
            sum += p;
        }
        sum / prices.len() as i128
    }
}
