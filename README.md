# Zenith

A dynamic liquidity protocol on Solana — three exchange engines, a shared
exact-math core, a TypeScript SDK, and a web app.

- **zenith-amm** — a concentrated-liquidity AMM (sqrt-price curve, per-position
  liquidity, dynamic fees, position NFTs).
- **zenith-dlmm** — a liquidity-book DEX (discrete price bins, zero in-bin
  slippage, Spot/Curve/BidAsk deposits, volatility-driven dynamic fees, TWAP
  oracle).
- **zenith-camm** — a full-range constant-product AMM (`x·y=k`, fungible LP
  shares) whose idle reserves earn yield.
- **zenith-math** — the shared fixed-point core (Q64.64, U256, sqrt-price,
  bin-price, constant-product) that every program and the SDK compute against,
  bit-for-bit.
- **@zenith/sdk** — TypeScript: state fetch, exact-math quoting, transaction
  builders. The SDK mirrors the on-chain integer math exactly, so a quote equals
  the realized on-chain amount.
- **app** — Vite + React frontend (wallet connect, swap, pools, positions, and a
  yield tab).

> **Target network: Solana devnet.**

### Contents

- [What Zenith is](#what-zenith-is)
- [The three engines](#the-three-engines)
- [Repository layout](#repository-layout)
- [Architecture — one swap, end to end](#architecture--one-swap-end-to-end)
- [On-chain instruction sets](#on-chain-instruction-sets)
- [The shared math core](#the-shared-math-core)
- [SDK](#sdk)
- [Devnet deployment](#devnet-deployment)
- [Build & test](#build--test)
- [Project status](#project-status)
- [Design notes](#design-notes)
- [Toolchain](#toolchain)
- [License](#license)

### What Zenith is

Zenith is a from-scratch implementation of three modern AMM designs, built to be
read as much as run. Every money path is written in checked integer math (no
floats on-chain, no floats in the SDK quote), reviewed adversarially, and
covered by reference vectors plus on-chain lifecycle and end-to-end tests.

All three engines are deployed to devnet and exercised end-to-end by the scripts
in `sdk/scripts/`: for each, the realized on-chain result is asserted to equal
the SDK quote exactly. Each engine ships a program, an SDK namespace, and a
frontend tab.

A companion, browsable explainer lives at `docs/zenith-explained.html` — it
walks Solana's account model and then exactly how Zenith uses each piece,
including a high-level design diagram of the full user flow.

### The three engines

#### zenith-amm — concentrated liquidity

Liquidity providers pick a price range; capital is only active inside that
range, so it is far denser than a constant-product pool.

- **Price as √price in Q64.64** — the curve tracks `√price`, so the swap step is
  a division, not a square root, and stays exact in integer math.
- **Per-position accounting** — each position tracks its liquidity `L` and a
  fee-growth checkpoint; fees owed are `L · (growth − checkpoint)`.
- **Dynamic fees** — a base fee plus a scheduler and a volatility surcharge.
- **Protocol / partner fee split** — a configurable cut routed to protocol and
  optional partner, claimable separately from LP fees.
- **Fee compounding** — a position can fold its owed fees back into liquidity.

#### zenith-dlmm — liquidity book (bins)

Price is quantized into discrete **bins** spaced `bin_step` bps apart; liquidity
in the active bin trades at a single price with **zero in-bin slippage**
(constant-sum), and swaps walk across bins.

- **Bins & bin arrays** — `bin_price(bin_id) = (1 + bin_step/10000)^bin_id` in
  Q64.64; bins are grouped into fixed-size `BinArray` accounts.
- **Deposit strategies** — `Spot` (uniform), `Curve` (concentrated at the active
  bin), `BidAsk` (concentrated at the range edges).
- **Volatility dynamic fee** — a variance accumulator driven by how far the
  active bin moves per swap, with filter/decay windows; the surcharge is
  computed pre-swap (no circularity) and capped.
- **Per-bin LP fee growth** — fees accrue per bin per share; positions claim
  without withdrawing liquidity.
- **TWAP oracle** — a ring buffer of cumulative-active-bin observations, floored
  via `div_euclid` for negative bins.

#### zenith-camm — full-range + yield

A classic constant-product pool (`reserve_a · reserve_b = k`) with a single
**fungible LP-share mint** — no ranges, no position NFTs, so being an LP is
passive: deposit both tokens, hold LP tokens, redeem any time.

- **Constant-product curve** — exact-in / exact-out on `x·y=k`; the fee is taken
  from the input and splits into a protocol cut and an LP cut that stays in the
  reserve, compounding into `k` for every LP.
- **Fungible shares** — the first deposit bootstraps with the geometric mean
  `√(a·b)` and permanently locks a minimum-liquidity floor (defusing the
  share-inflation attack); later deposits are trimmed to the pool ratio and mint
  shares proportionally.
- **Idle-reserve yield** — a full-range pool keeps most of its capital far from
  the price, so that idle capital is deployed to earn yield (a mock lending
  market on devnet): accrued yield is paid into the reserves, raising every LP's
  share value. A solvency buffer stays in the pool and the deployed principal
  never leaves the reserve vault, so swaps and withdrawals are always solvent.

### Repository layout

```
zenith/
├─ programs/
│  ├─ zenith-amm/          # concentrated-liquidity AMM (Rust + Anchor 0.31)
│  │  ├─ src/instructions/ # create_config, initialize_pool, create_position,
│  │  │                    #   modify_liquidity, swap, claim_*_fee, close_position
│  │  ├─ src/state/        # zero-copy Config / Pool / Position
│  │  └─ tests/            # fee-engine + golden math/account-byte vectors
│  ├─ zenith-dlmm/         # liquidity-book bin DEX (Rust + Anchor 0.31)
│  │  ├─ src/instructions/ # initialize_lb_pair, initialize_bin_array,
│  │  │                    #   initialize_position, add/remove_liquidity, swap,
│  │  │                    #   claim_fee, claim_protocol_fee, initialize_oracle
│  │  ├─ src/state/        # LbPair / BinArray / Position / Oracle (zero-copy)
│  │  └─ tests/            # fee / volatility / accrual / TWAP vectors + fuzz
│  └─ zenith-camm/         # full-range constant-product + yield (Rust + Anchor)
│     ├─ src/instructions/ # initialize_pool, add/remove_liquidity, swap,
│     │                    #   initialize_yield, harvest_yield, rebalance_to_vault
│     ├─ src/state/        # zero-copy Pool
│     └─ fee.rs yield_math.rs
├─ crates/
│  └─ zenith-math/         # shared fixed-point: q64, u256, sqrt_price, bin_price,
│                          #   constant_product (curve + LP shares)
├─ sdk/                    # @zenith/sdk (TypeScript)
│  ├─ src/                 # flat (amm) + dlmm/ + camm/ namespaces
│  └─ scripts/             # devnet end-to-end checks (swap/lp/claim/yield/seed/mint)
├─ app/                    # Vite + React frontend
│  └─ src/*-devnet.json    # live devnet program + pool + mint addresses per engine
├─ tests/                  # on-chain lifecycle tests (solana-program-test)
│  ├─ amm-integration/
│  └─ dlmm-integration/
└─ docs/                   # PRD, research notes, and the browsable explainer
```

### Architecture — one swap, end to end

Five tiers. The browser never does money math and the program never does
floating point; the SDK is the exact-integer bridge between them.

```
①  You + wallet     ·  pick a pair + amount, later sign
        ↓              amount in
②  Frontend (app/)  ·  React screens: Swap · Pools · Positions · DLMM · Yield
        ↓              "quote this", then "build the tx"
③  @zenith/sdk      ·  fetch pool state → quote (exact integer math)
        ↓              → build instruction (accounts + args)
④  Solana cluster   ·  verify sig · load declared accounts · run
        ↓              runtime invokes the program
⑤  zenith engine    ·  checks the math, writes pool / position state
        ↓              CPI to move tokens
⑥  SPL Token        ·  transfers between your ATAs and the pool's vaults

  ↺  result: out-tokens + confirmation flow back up →
     the app refreshes your balances and positions
```

The engine decides *how much* moves; it never custodies tokens — it CPIs into
the SPL Token program to move coins between your ATAs and the pool's PDA-owned
vaults. Separation of decision and custody.

The same shape serves add/remove liquidity, fee claims, and yield harvest — only
the instruction changes. A rendered version of this diagram is in
`docs/zenith-explained.html`.

### On-chain instruction sets

#### zenith-amm

| Instruction | Purpose |
| --- | --- |
| `create_config` | Create a fee/parameter config (admin). |
| `initialize_pool` | Create a pool for a mint pair at an initial price. |
| `create_position` | Open a position (NFT-custodied) over a price range. |
| `modify_liquidity` | Add or remove liquidity for a position. |
| `swap` | Exact-in / exact-out swap along the sqrt-price curve. |
| `claim_position_fee` | LP claims fees owed to a position. |
| `claim_protocol_fee` | Protocol claims its accrued cut. |
| `claim_partner_fee` | Partner claims its accrued cut. |
| `set_position_compounding` | Toggle fee auto-compounding for a position. |
| `close_position` | Close an empty position and reclaim rent. |

#### zenith-dlmm

| Instruction | Purpose |
| --- | --- |
| `initialize_lb_pair` | Create a liquidity-book pair (bin_step, active bin, fees). |
| `initialize_bin_array` | Create a bin-array account covering a bin range. |
| `initialize_position` | Open a position over a bin range. |
| `add_liquidity` | Deposit with a Spot / Curve / BidAsk strategy. |
| `remove_liquidity` | Withdraw pro-rata; zeroes emptied bins. |
| `swap` | Walk bins (constant-sum), crossing arrays as needed. |
| `claim_fee` | LP claims per-bin fees without withdrawing. |
| `claim_protocol_fee` | Creator claims the protocol cut. |
| `initialize_oracle` | Create the TWAP oracle ring buffer for the pair. |
| `close_position` | Close an empty, fully-settled position. |

#### zenith-camm

| Instruction | Purpose |
| --- | --- |
| `initialize_pool` | Create a pool (reserves, LP mint, locked-liquidity account). |
| `add_liquidity` | Deposit both tokens, mint LP shares (ratio-trimmed). |
| `remove_liquidity` | Burn LP shares, withdraw pro-rata reserves. |
| `swap` | Exact-in / exact-out along the `x·y=k` curve. |
| `initialize_yield` | Configure the yield engine + create funded yield sources. |
| `harvest_yield` | Pay accrued yield into the reserves (permissionless). |
| `rebalance_to_vault` | Harvest, then re-mark the idle reserve as deployed. |

### The shared math core

`crates/zenith-math` is the single source of numerical truth. Every on-chain
program and the SDK compute against the same definitions, so quotes are exact.

| Module | What it provides |
| --- | --- |
| `q64` | Q64.64 fixed-point: mul/div with explicit rounding, `from_ratio`. |
| `u256` | 256-bit `mul_div`, `mul_shr`, `shl_div` (overflow-safe intermediates). |
| `sqrt_price` | √price conversions, `deltaA/deltaB`, next-price, integer `isqrt`. |
| `bin_price` | `(1 + bin_step/10000)^bin_id` in Q64.64, strictly monotonic. |
| `constant_product` | `x·y=k` swap + LP-share math (geometric-mean bootstrap, proportional mint, pro-rata redeem). |

All rounding is directional and pool-favoring; every function is pinned by
reference vectors and property tests (round-trip, floor, monotonicity, no
over-budget liquidity, `k` non-decreasing).

### SDK

`@zenith/sdk` exposes PDA derivation, exact quote math, account decoders, and
transaction builders. Each engine has its own surface: the AMM is the flat
top-level export; the DLMM and CAMM engines are the `dlmm` and `camm` namespaces.
The math each uses is a direct port of `zenith-math`, verified bit-for-bit
against golden vectors, so a quote equals the on-chain result.

```ts
import { camm, ZenithConnection } from "@zenith/sdk";
import { Connection, PublicKey, clusterApiUrl } from "@solana/web3.js";

const connection = new Connection(clusterApiUrl("devnet"), "confirmed");
const zc = new ZenithConnection(connection, { commitment: "confirmed" });

// full-range engine: fetch the pool, quote a swap (matches on-chain exactly)
const pool = await camm.fetchPool(zc, new PublicKey(POOL));
const quote = camm.quoteSwap({
  pool,
  direction: camm.Direction.AtoB,
  mode: camm.SwapMode.ExactIn,
  amount: 1_000_000n,
});

// build the instruction, then sign + send with the wallet
const ix = camm.buildSwap({ /* pool, authority, vaults, user ATAs, amount, threshold */ });
```

The AMM namespace exposes `quoteSwap`, `buildSwap / buildAddLiquidity / …`, and
`decodePool / decodePosition / decodeConfig`; `dlmm` and `camm` mirror the shape
for their engines (`camm` adds `buildInitializeYield / buildHarvestYield /
buildRebalanceToVault` and the `accruedYield` helper).

### Devnet deployment

All three engines are live on devnet, each with a seeded pool.

| Engine | Program | Pool |
| --- | --- | --- |
| `zenith-amm` | `AA8cKcHQj63GEHRaLrrT87W1efRZ44U147JTCXC2Rmkq` | `AynzvqqaS2RiHs5Do63JsywXveoW8ob6BagLnknVkqCJ` (tUSDC/tUSDT) |
| `zenith-dlmm` | `7pxn8tEm44gXjfPH9YXsLywuYpAbgbxq9nPwG1XQczsz` | `5idsMpbewctoSp9J2LJVCvN18qciFdSBrveyqsmk1Yxb` (tBIN/tUSD) |
| `zenith-camm` | `CjjcK3rnskHswBpTgZquLGgS7P2QyzeaNwwe98FUUdy7` | `HmBojyfuF23uq71ZVGXW8jcMk2e8HUEtTsYujhFCYRXK` (tCP/tUSD) |

The frontend reads live addresses from `app/src/devnet.json`,
`app/src/dlmm-devnet.json`, and `app/src/camm-devnet.json`.

**End-to-end scripts** (sign with `~/.config/solana/id.json`, devnet SOL
required). Each seeds a market and asserts the realized on-chain result equals
the SDK quote exactly:

```sh
cd sdk
# per-engine seed + checks
npx tsx scripts/seed-devnet.ts       # AMM: mints, config, pool, liquidity
npx tsx scripts/swap-check.ts        # AMM: realized output == SDK quote
npx tsx scripts/camm-seed.ts         # CAMM: mints, pool, liquidity, yield
npx tsx scripts/camm-swap-check.ts   # CAMM: realized output == SDK quote
npx tsx scripts/camm-lp-check.ts     # CAMM: add/remove round-trip (no value leak)
npx tsx scripts/camm-yield-check.ts  # CAMM: harvest pays yield into the reserves
npx tsx scripts/dlmm-swap-check.ts   # DLMM: realized output == SDK quote
npx tsx scripts/mint-to.ts <pubkey>  # mint test tokens to a wallet
```

### Build & test

```sh
# Rust — host unit + reference vectors
cargo test                              # all crates
cargo test -p zenith-math               # math core (curve, sqrt, bin, u256)
cargo test -p zenith-amm                # AMM host tests
cargo test -p zenith-dlmm               # DLMM host tests + fee/TWAP vectors
cargo test -p zenith-camm               # CAMM host tests (curve, fee, yield, layout)
cargo test -p amm-integration \
          -p dlmm-integration           # on-chain lifecycle (solana-program-test)

cargo build-sbf --manifest-path programs/zenith-amm/Cargo.toml
cargo build-sbf --manifest-path programs/zenith-dlmm/Cargo.toml
cargo build-sbf --manifest-path programs/zenith-camm/Cargo.toml
cargo fmt --all -- --check

# SDK
cd sdk && npm install && npm run build && npm test

# Frontend (devnet)
cd app && npm install && npm run dev    # http://localhost:5173
```

**Test inventory**

| Suite | Kind |
| --- | --- |
| `zenith-math` unit + `tests/vectors.rs` | reference vectors + property tests |
| `zenith-amm` unit + `fee_engine_vectors` + `golden_*` | fee engine + golden bytes/math |
| `zenith-dlmm` unit + `fee_vectors` | volatility / accrual / TWAP vectors + fuzz |
| `zenith-camm` unit | curve, fee split, yield accrual, PDA, account layout |
| `sdk` vitest | golden-vector parity (AMM, DLMM, CAMM curve + LP) + builders |
| `amm-integration/lifecycle` | full on-chain AMM lifecycle |
| `dlmm-integration/lifecycle` | init → add → multi-bin/cross-array swaps → fuzz → claim → close |

Integration crates run under `solana-program-test` (in-process bank) and are
excluded from the `-p`-scoped program CI jobs; run them explicitly as above.

Optionally point the frontend at a dedicated RPC by setting
`VITE_RPC_ENDPOINT` in `app/.env` (falls back to the public devnet cluster).

### Project status

| Component | State |
| --- | --- |
| `zenith-math` | Complete — vectors + property tests |
| `zenith-amm` | Complete — deployed to devnet, end-to-end verified |
| `zenith-dlmm` | Complete — deployed to devnet, SDK + UI, end-to-end verified |
| `zenith-camm` | Complete — deployed to devnet, SDK + UI, end-to-end verified |
| `@zenith/sdk` | All three engines (quote, builders, decoders) |
| `app` | All three engines (swap, pools, positions, DLMM, yield) |

Every engine ships program + SDK + frontend, live on devnet with the realized
on-chain result asserted equal to the SDK quote. Optional next work: Jupiter
integration and release tooling.

### Design notes

- **No floats.** All value math is checked integer / fixed-point, on-chain and in
  the SDK quote. Rounding is directional and always pool-favoring.
- **Zero-copy accounts.** State uses `bytemuck` `Pod` layouts (`repr(C)`,
  descending-alignment fields, 16-byte-multiple sizes).
- **Custody separation.** Programs never hold tokens; transfers go through SPL
  Token via CPI to PDA-owned vaults.
- **Adversarially reviewed.** Each money handler was reviewed for token
  conservation, no over-withdraw, fee leakage, and insolvency before merge.
- Runs on **Solana devnet**.

### Toolchain

- Rust + Anchor `0.31` (on-chain programs)
- Solana CLI + `cargo build-sbf` (devnet)
- Node + TypeScript (SDK, app); `@solana/web3.js` `^1.95`

### License

MIT
