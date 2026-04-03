#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype,
    Address, Env, Symbol, Vec, symbol_short,
    panic_with_error,
};

#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum RouterError {
    NotInitialized    = 1,
    NoRouteFound      = 2,
    SlippageExceeded  = 3,
    InsufficientOutput= 4,
    DeadlineExceeded  = 5,
    InvalidPath       = 6,
}

#[contracttype]
pub enum RouterKey {
    Config,
    Pool(Address, Address),  // (token_a, token_b) -> pool address
    OracleAddress,
}

#[contracttype]
#[derive(Clone)]
pub struct RouterConfig {
    pub admin: Address,
    pub oracle: Address,
    pub max_hops: u32,           // maximum number of hops in a route
    pub protocol_fee_bps: u32,   // protocol fee on top of pool fee
    pub initialized: bool,
}

#[contracttype]
#[derive(Clone)]
pub struct SwapRoute {
    pub path: Vec<Address>,      // token path e.g. [USDC, bAAPL]
    pub pools: Vec<Address>,     // pool addresses for each hop
    pub expected_output: i128,
    pub price_impact_bps: u32,
}

#[contracttype]
pub struct ExactInputParams {
    pub sender: Address,
    pub recipient: Address,
    pub token_in: Address,
    pub token_out: Address,
    pub amount_in: i128,
    pub amount_out_min: i128,    // slippage protection
    pub deadline: u64,
}

#[contracttype]
pub struct ExactOutputParams {
    pub sender: Address,
    pub recipient: Address,
    pub token_in: Address,
    pub token_out: Address,
    pub amount_out: i128,
    pub amount_in_max: i128,     // slippage protection
    pub deadline: u64,
}

#[contract]
pub struct CeresRouter;

#[contractimpl]
impl CeresRouter {

    /// Initialize router with oracle and pool registry.
    pub fn initialize(env: Env, admin: Address, oracle: Address) {
        if env.storage().instance().has(&RouterKey::Config) {
            panic_with_error!(&env, RouterError::NotInitialized);
        }

        admin.require_auth();

        let config = RouterConfig {
            admin,
            oracle,
            max_hops: 3,
            protocol_fee_bps: 0, // router is free — earns nothing directly
            initialized: true,
        };

        env.storage().instance().set(&RouterKey::Config, &config);
    }

    /// Register a pool for a token pair.
    pub fn register_pool(env: Env, token_a: Address, token_b: Address, pool: Address) {
        let config: RouterConfig = env.storage().instance()
            .get(&RouterKey::Config)
            .unwrap_or_else(|| panic_with_error!(&env, RouterError::NotInitialized));

        config.admin.require_auth();
        env.storage().persistent().set(&RouterKey::Pool(token_a, token_b), &pool);
    }

    /// Get the best route and expected output for a swap.
    /// Does not execute — read-only quote function.
    pub fn get_quote(
        env: Env,
        token_in: Address,
        token_out: Address,
        amount_in: i128,
    ) -> SwapRoute {
        let config: RouterConfig = env.storage().instance()
            .get(&RouterKey::Config)
            .unwrap_or_else(|| panic_with_error!(&env, RouterError::NotInitialized));

        // Try direct route first
        if let Some(pool) = env.storage().persistent()
            .get::<RouterKey, Address>(&RouterKey::Pool(token_in.clone(), token_out.clone()))
        {
            let expected = Self::simulate_swap(&env, &pool, amount_in, true);
            let price_impact = Self::calculate_price_impact(&env, &config.oracle, &token_out, amount_in, expected);

            let mut path = Vec::new(&env);
            path.push_back(token_in);
            path.push_back(token_out);

            let mut pools = Vec::new(&env);
            pools.push_back(pool);

            return SwapRoute { path, pools, expected_output: expected, price_impact_bps: price_impact };
        }

        // No direct route found
        panic_with_error!(&env, RouterError::NoRouteFound);
    }

    /// Execute a swap with exact input amount.
    /// Returns the actual output amount received.
    pub fn swap_exact_input(env: Env, params: ExactInputParams) -> i128 {
        params.sender.require_auth();
        Self::check_deadline(&env, params.deadline);

        let route = Self::get_quote(
            env.clone(),
            params.token_in.clone(),
            params.token_out.clone(),
            params.amount_in,
        );

        if route.expected_output < params.amount_out_min {
            panic_with_error!(&env, RouterError::SlippageExceeded);
        }

        // Execute through the route's pools
        // Full implementation chains swaps through each pool in the route
        let amount_out = Self::execute_route(&env, &route, &params.sender, &params.recipient, params.amount_in);

        env.events().publish(
            (symbol_short!("swap"), symbol_short!("exact_in")),
            (params.sender, params.amount_in, amount_out),
        );

        amount_out
    }

    /// Execute a swap targeting an exact output amount.
    /// Returns the actual input amount spent.
    pub fn swap_exact_output(env: Env, params: ExactOutputParams) -> i128 {
        params.sender.require_auth();
        Self::check_deadline(&env, params.deadline);

        // Calculate required input for desired output
        let required_input = Self::compute_required_input(
            &env,
            &params.token_in,
            &params.token_out,
            params.amount_out,
        );

        if required_input > params.amount_in_max {
            panic_with_error!(&env, RouterError::SlippageExceeded);
        }

        let route = Self::get_quote(
            env.clone(),
            params.token_in.clone(),
            params.token_out.clone(),
            required_input,
        );

        let amount_out = Self::execute_route(&env, &route, &params.sender, &params.recipient, required_input);

        env.events().publish(
            (symbol_short!("swap"), symbol_short!("exact_out")),
            (params.sender, required_input, amount_out),
        );

        required_input
    }

    /// Get the registered pool for a token pair.
    pub fn get_pool(env: Env, token_a: Address, token_b: Address) -> Option<Address> {
        env.storage().persistent()
            .get(&RouterKey::Pool(token_a, token_b))
    }

    // ── INTERNAL ─────────────────────────────────────────────────────────────

    fn check_deadline(env: &Env, deadline: u64) {
        if env.ledger().timestamp() > deadline {
            panic_with_error!(env, RouterError::DeadlineExceeded);
        }
    }

    fn simulate_swap(_env: &Env, _pool: &Address, amount_in: i128, _zero_for_one: bool) -> i128 {
        // Full implementation calls pool contract's quote function
        (amount_in * 997) / 1000
    }

    fn calculate_price_impact(
        _env: &Env,
        _oracle: &Address,
        _token_out: &Address,
        _amount_in: i128,
        _amount_out: i128,
    ) -> u32 {
        // Full implementation compares execution price vs oracle price
        // Returns deviation in basis points
        10 // placeholder: 0.1% price impact
    }

    fn execute_route(
        _env: &Env,
        route: &SwapRoute,
        _sender: &Address,
        _recipient: &Address,
        amount_in: i128,
    ) -> i128 {
        // Full implementation calls pool.swap() for each hop in the route
        route.expected_output
    }

    fn compute_required_input(
        env: &Env,
        token_in: &Address,
        token_out: &Address,
        amount_out: i128,
    ) -> i128 {
        // Full implementation uses inverse swap math
        (amount_out * 1003) / 1000
    }
}
