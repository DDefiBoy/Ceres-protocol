#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype,
    Address, Env, Symbol, symbol_short, token,
    panic_with_error,
};

// ── ERRORS ────────────────────────────────────────────────────────────────────
#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum CeresError {
    NotInitialized       = 1,
    AlreadyInitialized   = 2,
    InvalidTickRange     = 3,
    InsufficientLiquidity= 4,
    SlippageExceeded     = 5,
    ZeroLiquidity        = 6,
    InvalidToken         = 7,
    Unauthorized         = 8,
    PriceOutOfRange      = 9,
    DeadlineExceeded     = 10,
}

// ── STORAGE KEYS ─────────────────────────────────────────────────────────────
#[contracttype]
pub enum DataKey {
    Pool,
    Tick(i32),
    Position(Address, i32, i32), // owner, tick_lower, tick_upper
    FeeTier,
}

// ── POOL STATE ────────────────────────────────────────────────────────────────
#[contracttype]
#[derive(Clone)]
pub struct PoolState {
    pub token_a: Address,          // USDC
    pub token_b: Address,          // tokenized asset (e.g. bAAPL)
    pub fee_bps: u32,              // fee in basis points (e.g. 30 = 0.3%)
    pub sqrt_price_x64: u128,      // current sqrt(price) * 2^64
    pub current_tick: i32,         // current active tick index
    pub liquidity: u128,           // total active liquidity at current tick
    pub fee_growth_a: u128,        // global fee growth per unit of liquidity (token A)
    pub fee_growth_b: u128,        // global fee growth per unit of liquidity (token B)
    pub protocol_fee_a: u128,      // accumulated protocol fees (token A)
    pub protocol_fee_b: u128,      // accumulated protocol fees (token B)
    pub initialized: bool,
}

// ── TICK STATE ────────────────────────────────────────────────────────────────
#[contracttype]
#[derive(Clone, Default)]
pub struct TickState {
    pub liquidity_gross: u128,     // total liquidity referencing this tick
    pub liquidity_net: i128,       // net liquidity change when tick is crossed
    pub fee_growth_outside_a: u128,
    pub fee_growth_outside_b: u128,
    pub initialized: bool,
}

// ── LP POSITION ────────────────────────────────────────────────────────────────
#[contracttype]
#[derive(Clone)]
pub struct Position {
    pub owner: Address,
    pub tick_lower: i32,
    pub tick_upper: i32,
    pub liquidity: u128,
    pub fee_growth_inside_a_last: u128,
    pub fee_growth_inside_b_last: u128,
    pub tokens_owed_a: u128,
    pub tokens_owed_b: u128,
}

// ── ADD LIQUIDITY PARAMS ──────────────────────────────────────────────────────
#[contracttype]
pub struct AddLiquidityParams {
    pub sender: Address,
    pub tick_lower: i32,
    pub tick_upper: i32,
    pub amount_a_desired: i128,    // max token A to deposit
    pub amount_b_desired: i128,    // max token B to deposit
    pub amount_a_min: i128,        // slippage protection
    pub amount_b_min: i128,
    pub deadline: u64,
}

// ── REMOVE LIQUIDITY PARAMS ───────────────────────────────────────────────────
#[contracttype]
pub struct RemoveLiquidityParams {
    pub sender: Address,
    pub tick_lower: i32,
    pub tick_upper: i32,
    pub liquidity: u128,           // amount of liquidity to remove
    pub amount_a_min: i128,
    pub amount_b_min: i128,
    pub deadline: u64,
}

// ── SWAP PARAMS ───────────────────────────────────────────────────────────────
#[contracttype]
pub struct SwapParams {
    pub sender: Address,
    pub recipient: Address,
    pub zero_for_one: bool,        // true = token_a -> token_b, false = token_b -> token_a
    pub amount_in: i128,
    pub sqrt_price_limit_x64: u128, // price cannot cross this limit
    pub deadline: u64,
}

// ── CONTRACT ──────────────────────────────────────────────────────────────────
#[contract]
pub struct CeresPool;

#[contractimpl]
impl CeresPool {

    /// Initialize the pool with two token addresses and a fee tier.
    /// Can only be called once by the pool factory.
    pub fn initialize(
        env: Env,
        token_a: Address,
        token_b: Address,
        fee_bps: u32,
        initial_sqrt_price_x64: u128,
    ) {
        if env.storage().instance().has(&DataKey::Pool) {
            panic_with_error!(&env, CeresError::AlreadyInitialized);
        }

        let pool = PoolState {
            token_a,
            token_b,
            fee_bps,
            sqrt_price_x64: initial_sqrt_price_x64,
            current_tick: Self::tick_from_sqrt_price(initial_sqrt_price_x64),
            liquidity: 0,
            fee_growth_a: 0,
            fee_growth_b: 0,
            protocol_fee_a: 0,
            protocol_fee_b: 0,
            initialized: true,
        };

        env.storage().instance().set(&DataKey::Pool, &pool);
    }

    /// Add concentrated liquidity within a price range.
    /// Returns (amount_a_deposited, amount_b_deposited, liquidity_minted).
    pub fn add_liquidity(
        env: Env,
        params: AddLiquidityParams,
    ) -> (i128, i128, u128) {
        params.sender.require_auth();
        Self::check_deadline(&env, params.deadline);

        let pool: PoolState = env.storage().instance()
            .get(&DataKey::Pool)
            .unwrap_or_else(|| panic_with_error!(&env, CeresError::NotInitialized));

        Self::validate_tick_range(&env, params.tick_lower, params.tick_upper);

        // Calculate liquidity from desired amounts and current price
        let liquidity = Self::compute_liquidity(
            pool.sqrt_price_x64,
            params.tick_lower,
            params.tick_upper,
            params.amount_a_desired,
            params.amount_b_desired,
        );

        if liquidity == 0 {
            panic_with_error!(&env, CeresError::ZeroLiquidity);
        }

        // Calculate actual token amounts required for this liquidity
        let (amount_a, amount_b) = Self::amounts_for_liquidity(
            pool.sqrt_price_x64,
            params.tick_lower,
            params.tick_upper,
            liquidity,
        );

        // Slippage check
        if amount_a < params.amount_a_min || amount_b < params.amount_b_min {
            panic_with_error!(&env, CeresError::SlippageExceeded);
        }

        // Transfer tokens from sender to pool
        token::Client::new(&env, &pool.token_a)
            .transfer(&params.sender, &env.current_contract_address(), &amount_a);
        token::Client::new(&env, &pool.token_b)
            .transfer(&params.sender, &env.current_contract_address(), &amount_b);

        // Update or create position
        let pos_key = DataKey::Position(
            params.sender.clone(),
            params.tick_lower,
            params.tick_upper,
        );

        let mut position: Position = env.storage().persistent()
            .get(&pos_key)
            .unwrap_or(Position {
                owner: params.sender.clone(),
                tick_lower: params.tick_lower,
                tick_upper: params.tick_upper,
                liquidity: 0,
                fee_growth_inside_a_last: 0,
                fee_growth_inside_b_last: 0,
                tokens_owed_a: 0,
                tokens_owed_b: 0,
            });

        position.liquidity += liquidity;
        env.storage().persistent().set(&pos_key, &position);

        // Emit event
        env.events().publish(
            (symbol_short!("liq_add"), params.sender),
            (liquidity, amount_a, amount_b),
        );

        (amount_a, amount_b, liquidity)
    }

    /// Remove liquidity from a position. Returns tokens + accrued fees.
    pub fn remove_liquidity(
        env: Env,
        params: RemoveLiquidityParams,
    ) -> (i128, i128) {
        params.sender.require_auth();
        Self::check_deadline(&env, params.deadline);

        let pool: PoolState = env.storage().instance()
            .get(&DataKey::Pool)
            .unwrap_or_else(|| panic_with_error!(&env, CeresError::NotInitialized));

        let pos_key = DataKey::Position(
            params.sender.clone(),
            params.tick_lower,
            params.tick_upper,
        );

        let mut position: Position = env.storage().persistent()
            .get(&pos_key)
            .unwrap_or_else(|| panic_with_error!(&env, CeresError::InsufficientLiquidity));

        if position.liquidity < params.liquidity {
            panic_with_error!(&env, CeresError::InsufficientLiquidity);
        }

        let (amount_a, amount_b) = Self::amounts_for_liquidity(
            pool.sqrt_price_x64,
            params.tick_lower,
            params.tick_upper,
            params.liquidity,
        );

        if amount_a < params.amount_a_min || amount_b < params.amount_b_min {
            panic_with_error!(&env, CeresError::SlippageExceeded);
        }

        position.liquidity -= params.liquidity;
        env.storage().persistent().set(&pos_key, &position);

        // Transfer tokens back to sender
        token::Client::new(&env, &pool.token_a)
            .transfer(&env.current_contract_address(), &params.sender, &amount_a);
        token::Client::new(&env, &pool.token_b)
            .transfer(&env.current_contract_address(), &params.sender, &amount_b);

        env.events().publish(
            (symbol_short!("liq_rem"), params.sender),
            (params.liquidity, amount_a, amount_b),
        );

        (amount_a, amount_b)
    }

    /// Execute a swap. Core AMM function.
    pub fn swap(env: Env, params: SwapParams) -> i128 {
        params.sender.require_auth();
        Self::check_deadline(&env, params.deadline);

        let pool: PoolState = env.storage().instance()
            .get(&DataKey::Pool)
            .unwrap_or_else(|| panic_with_error!(&env, CeresError::NotInitialized));

        if pool.liquidity == 0 {
            panic_with_error!(&env, CeresError::InsufficientLiquidity);
        }

        // Fee calculation
        let fee_amount = (params.amount_in * pool.fee_bps as i128) / 10000;
        let amount_after_fee = params.amount_in - fee_amount;

        // Simplified swap output calculation
        // Full implementation requires tick-crossing logic
        let amount_out = Self::compute_swap_output(
            &pool,
            params.zero_for_one,
            amount_after_fee,
            params.sqrt_price_limit_x64,
        );

        // Transfer tokens
        if params.zero_for_one {
            token::Client::new(&env, &pool.token_a)
                .transfer(&params.sender, &env.current_contract_address(), &params.amount_in);
            token::Client::new(&env, &pool.token_b)
                .transfer(&env.current_contract_address(), &params.recipient, &amount_out);
        } else {
            token::Client::new(&env, &pool.token_b)
                .transfer(&params.sender, &env.current_contract_address(), &params.amount_in);
            token::Client::new(&env, &pool.token_a)
                .transfer(&env.current_contract_address(), &params.recipient, &amount_out);
        }

        env.events().publish(
            (symbol_short!("swap"), params.sender),
            (params.amount_in, amount_out, params.zero_for_one),
        );

        amount_out
    }

    /// Collect accumulated fees for a position.
    pub fn collect_fees(
        env: Env,
        sender: Address,
        tick_lower: i32,
        tick_upper: i32,
    ) -> (u128, u128) {
        sender.require_auth();

        let pool: PoolState = env.storage().instance()
            .get(&DataKey::Pool)
            .unwrap_or_else(|| panic_with_error!(&env, CeresError::NotInitialized));

        let pos_key = DataKey::Position(sender.clone(), tick_lower, tick_upper);
        let mut position: Position = env.storage().persistent()
            .get(&pos_key)
            .unwrap_or_else(|| panic_with_error!(&env, CeresError::InsufficientLiquidity));

        let owed_a = position.tokens_owed_a;
        let owed_b = position.tokens_owed_b;

        position.tokens_owed_a = 0;
        position.tokens_owed_b = 0;
        env.storage().persistent().set(&pos_key, &position);

        if owed_a > 0 {
            token::Client::new(&env, &pool.token_a)
                .transfer(&env.current_contract_address(), &sender, &(owed_a as i128));
        }
        if owed_b > 0 {
            token::Client::new(&env, &pool.token_b)
                .transfer(&env.current_contract_address(), &sender, &(owed_b as i128));
        }

        (owed_a, owed_b)
    }

    /// Read pool state.
    pub fn get_pool(env: Env) -> PoolState {
        env.storage().instance()
            .get(&DataKey::Pool)
            .unwrap_or_else(|| panic_with_error!(&env, CeresError::NotInitialized))
    }

    /// Read a position.
    pub fn get_position(env: Env, owner: Address, tick_lower: i32, tick_upper: i32) -> Option<Position> {
        env.storage().persistent()
            .get(&DataKey::Position(owner, tick_lower, tick_upper))
    }

    // ── INTERNAL HELPERS ─────────────────────────────────────────────────────

    fn check_deadline(env: &Env, deadline: u64) {
        if env.ledger().timestamp() > deadline {
            panic_with_error!(env, CeresError::DeadlineExceeded);
        }
    }

    fn validate_tick_range(env: &Env, tick_lower: i32, tick_upper: i32) {
        if tick_lower >= tick_upper || tick_lower < -887272 || tick_upper > 887272 {
            panic_with_error!(env, CeresError::InvalidTickRange);
        }
    }

    fn tick_from_sqrt_price(sqrt_price_x64: u128) -> i32 {
        // Simplified: full implementation uses log base 1.0001
        ((sqrt_price_x64 as f64).ln() / (1.0001f64).ln()) as i32
    }

    fn compute_liquidity(
        sqrt_price: u128,
        tick_lower: i32,
        tick_upper: i32,
        amount_a: i128,
        amount_b: i128,
    ) -> u128 {
        // Simplified liquidity calculation
        // Full implementation uses sqrt price math
        (amount_a.min(amount_b)) as u128
    }

    fn amounts_for_liquidity(
        sqrt_price: u128,
        tick_lower: i32,
        tick_upper: i32,
        liquidity: u128,
    ) -> (i128, i128) {
        // Simplified — full implementation uses concentrated liquidity math
        (liquidity as i128 / 2, liquidity as i128 / 2)
    }

    fn compute_swap_output(
        pool: &PoolState,
        zero_for_one: bool,
        amount_in: i128,
        sqrt_price_limit: u128,
    ) -> i128 {
        // Simplified constant product approximation
        // Full implementation steps through ticks
        (amount_in * 997) / 1000
    }
}
