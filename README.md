# Ceres Protocol

> Open liquidity infrastructure for Stellar's RWA ecosystem.

Ceres is a permissionless concentrated liquidity protocol built on Soroban — Stellar's smart contract platform. It provides the foundational infrastructure that Stellar's RWA ecosystem currently lacks: deep liquidity pools, reliable real-time price feeds, and intelligent trade routing.

**Any wallet, trading platform, or DeFi protocol building on Stellar can integrate Ceres without permission.**

---

## What Ceres Provides

- **Concentrated Liquidity Pools** — capital-efficient AMM pools optimized for RWA and stablecoin pairs
- **Oracle Module** — decentralized real-time price feeds for tokenized stocks, commodities, and yield assets
- **Routing Engine** — smart order routing across all available Stellar liquidity sources
- **Open SDK** — JavaScript/TypeScript SDK for any app to integrate in minutes

---

## Repo Structure

```
ceres-protocol/
├── contracts/
│   ├── pool/           # Concentrated liquidity pool contracts (Rust/Soroban)
│   ├── oracle/         # Oracle aggregator + keeper registry
│   ├── router/         # Smart order routing engine
│   ├── fee-collector/  # Fee distribution logic
│   └── governance/     # Protocol governance (Phase 2)
├── sdk/
│   ├── src/            # TypeScript SDK source
│   └── tests/          # SDK integration tests
├── scripts/            # Deployment and admin scripts
├── docs/               # Protocol documentation
└── .github/
    └── workflows/      # CI/CD pipelines
```

---

## Getting Started

### Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) (latest stable)
- [Soroban CLI](https://soroban.stellar.org/docs/getting-started/setup)
- [Node.js](https://nodejs.org/) v18+ (for SDK)

### Install Soroban CLI

```bash
cargo install --locked soroban-cli
```

### Clone and Build

```bash
git clone https://github.com/DDefiboy/ceres-protocol.git
cd ceres-protocol
```

### Build Contracts

```bash
cd contracts/pool
cargo build --target wasm32-unknown-unknown --release
```

### Run Contract Tests

```bash
cargo test
```

### Install SDK Dependencies

```bash
cd sdk
npm install
npm run build
```

---

## Contracts

| Contract | Description | Status |
|---|---|---|
| `pool` | Concentrated liquidity AMM core | In Development |
| `oracle` | Price feed aggregator + keeper registry | In Development |
| `router` | Optimal swap path execution | In Development |
| `fee-collector` | LP and protocol fee distribution | In Development |
| `governance` | Protocol parameter governance | Planned (Phase 2) |

---

## SDK Usage

```typescript
import { Ceres } from '@ceres-protocol/sdk';

const ceres = new Ceres({ network: 'testnet' });

// Get oracle price
const price = await ceres.oracle.getPrice('bAAPL');

// Get swap quote
const quote = await ceres.router.getQuote({
  tokenIn: 'USDC',
  tokenOut: 'bAAPL',
  amountIn: 1000_0000000,
  slippage: 0.005,
});

// Add liquidity
const tx = await ceres.pools.buildAddLiquidityTransaction({
  pool: 'USDC/bAAPL',
  sender: walletAddress,
  tickLower: -500,
  tickUpper: 500,
  amountUSDC: 10000_0000000,
});
```

---

## Architecture

```
┌─────────────────────────────────────────────┐
│              Applications                    │
│   StellarTrade  |  Wallets  |  DeFi Protocols│
└──────────────────┬──────────────────────────┘
                   │
┌──────────────────▼──────────────────────────┐
│            Ceres Protocol                    │
│  ┌──────────┐ ┌─────────┐ ┌──────────────┐  │
│  │  Oracle  │ │  Pools  │ │    Router    │  │
│  └──────────┘ └─────────┘ └──────────────┘  │
└──────────────────┬──────────────────────────┘
                   │
┌──────────────────▼──────────────────────────┐
│         Stellar Network                      │
│   Soroban  |  SDEX  |  USDC  |  Anchors     │
└─────────────────────────────────────────────┘
```


## Contributing

Ceres is open infrastructure. Contributions are welcome.

1. Fork the repo
2. Create a feature branch (`git checkout -b feature/oracle-module`)
3. Commit your changes (`git commit -m 'add oracle keeper registry'`)
4. Push and open a Pull Request

Please read [CONTRIBUTING.md](./docs/CONTRIBUTING.md) before submitting.

---

## Security

Found a vulnerability? Do not open a public issue. Email: **security@ceres-protocol.xyz** (placeholder — update before launch).

See [SECURITY.md](./docs/SECURITY.md) for our full disclosure policy.

---

## License

MIT License. See [LICENSE](./LICENSE).

---


*Ceres — open liquidity infrastructure for Stellar...*
