# Zenith

A dynamic liquidity protocol on Solana:

- **zenith-amm** — a concentrated-liquidity AMM (sqrt-price, position NFTs).
- **zenith-dlmm** — a bin-based liquidity-book DEX (discrete price bins, dynamic fees).
- **@zenith/sdk** — TypeScript SDK (state fetch, exact-math quoting, transaction builders).
- **app** — Vite + React web app.

> Target network: **Solana devnet**. Not for mainnet / real funds.

## Layout

```
zenith/
├─ programs/
│  ├─ zenith-amm/     # concentrated-liquidity AMM (Rust + Anchor)
│  └─ zenith-dlmm/    # liquidity-book bin DEX (Rust + Anchor)
├─ crates/
│  └─ zenith-math/    # shared fixed-point math (Q64.64, U256)
├─ sdk/               # @zenith/sdk (TypeScript)
├─ app/               # Vite + React frontend
├─ tests/             # Anchor integration tests
└─ docs/              # PRD + architecture
```

## Toolchain

- Rust + Anchor (on-chain programs)
- Solana CLI (devnet)
- Node + TypeScript (SDK, app)

## Status

Early development.

## License

MIT
