//! Program events.

use anchor_lang::prelude::*;

/// Emitted when a pool is created and its first position opened.
#[event]
pub struct PoolInitialized {
    /// The new pool.
    pub pool: Pubkey,
    /// Token A (base) mint.
    pub token_a_mint: Pubkey,
    /// Token B (quote) mint.
    pub token_b_mint: Pubkey,
    /// Initial sqrt price (Q64.64 raw bits).
    pub sqrt_price: u128,
    /// Lower band bound (Q64.64 raw bits).
    pub sqrt_min_price: u128,
    /// Upper band bound (Q64.64 raw bits).
    pub sqrt_max_price: u128,
    /// Initial active liquidity seeded by the creator.
    pub liquidity: u128,
    /// The first position's NFT mint.
    pub position_nft_mint: Pubkey,
    /// Token A deposited to seed the position.
    pub amount_a: u64,
    /// Token B deposited to seed the position.
    pub amount_b: u64,
}

/// Emitted when an empty position is opened via `create_position`.
#[event]
pub struct PositionCreated {
    /// The pool the position belongs to.
    pub pool: Pubkey,
    /// The new position PDA.
    pub position: Pubkey,
    /// The NFT mint that represents ownership of the position.
    pub position_nft_mint: Pubkey,
    /// Account the NFT was minted to (the position owner).
    pub owner: Pubkey,
}

/// Emitted when liquidity is added to a position.
#[event]
pub struct LiquidityAdded {
    /// The pool.
    pub pool: Pubkey,
    /// The position the liquidity was added to.
    pub position: Pubkey,
    /// Liquidity `L` added.
    pub liquidity_delta: u128,
    /// Token A deposited.
    pub amount_a: u64,
    /// Token B deposited.
    pub amount_b: u64,
    /// Position's unlocked liquidity after the add.
    pub position_liquidity: u128,
    /// Pool's active liquidity after the add.
    pub pool_liquidity: u128,
}

/// Emitted when a position's accrued fees are claimed.
#[event]
pub struct FeesClaimed {
    /// The pool.
    pub pool: Pubkey,
    /// The position fees were claimed from.
    pub position: Pubkey,
    /// Token A fees paid out.
    pub amount_a: u64,
    /// Token B fees paid out.
    pub amount_b: u64,
}

/// Emitted on a swap.
#[event]
pub struct Swap {
    /// The pool.
    pub pool: Pubkey,
    /// `true` if the trader sold token A for token B (price fell).
    pub a_to_b: bool,
    /// Gross input paid by the trader (including fee).
    pub amount_in: u64,
    /// Output received by the trader.
    pub amount_out: u64,
    /// Total fee taken from the input.
    pub fee: u64,
    /// Portion of the fee routed to the protocol.
    pub protocol_fee: u64,
    /// Unspent input returned (nonzero only for partial fills).
    pub amount_remaining: u64,
    /// Pool sqrt-price after the swap (Q64.64 raw bits).
    pub sqrt_price: u128,
}

/// Emitted when liquidity is removed from a position.
#[event]
pub struct LiquidityRemoved {
    /// The pool.
    pub pool: Pubkey,
    /// The position the liquidity was removed from.
    pub position: Pubkey,
    /// Liquidity `L` removed.
    pub liquidity_delta: u128,
    /// Token A returned.
    pub amount_a: u64,
    /// Token B returned.
    pub amount_b: u64,
    /// Position's unlocked liquidity after the remove.
    pub position_liquidity: u128,
    /// Pool's active liquidity after the remove.
    pub pool_liquidity: u128,
}
