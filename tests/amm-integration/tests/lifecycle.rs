//! End-to-end M1 lifecycle test against the real compiled program, run on the
//! BPF VM via `solana-program-test`.
//!
//! Flow: create_config → initialize_pool (opens position 1) → create_position +
//! add_liquidity (position 2) → swap both directions (ExactIn + ExactOut) →
//! claim fees on both positions (asserting the proportional split) →
//! remove_all_liquidity → close_position. Asserts the price stays in band and
//! that fees split with liquidity.
//!
//! Local-only: build the BPF program first —
//!   cargo build-sbf --manifest-path programs/zenith-amm/Cargo.toml
//! then `cargo test -p zenith-amm --test integration`. The harness points
//! `SBF_OUT_DIR` at `target/deploy` so it loads `zenith_amm.so`.

use anchor_lang::{InstructionData, ToAccountMetas};
use solana_program_test::{BanksClient, ProgramTest};
use solana_sdk::{
    instruction::Instruction,
    program_pack::Pack,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    system_instruction,
    transaction::Transaction,
};
use spl_associated_token_account::{
    get_associated_token_address, instruction::create_associated_token_account,
};

use zenith_amm::math::{SwapDirection, SwapMode};

const Q64: u128 = 1u128 << 64;
const DECIMALS: u8 = 6;

/// Send a transaction signed by `signers`, panicking on failure.
async fn send(banks: &mut BanksClient, ixs: &[Instruction], payer: &Keypair, signers: &[&Keypair]) {
    let bh = banks.get_latest_blockhash().await.unwrap();
    let tx = Transaction::new_signed_with_payer(ixs, Some(&payer.pubkey()), signers, bh);
    banks.process_transaction(tx).await.expect("tx failed");
}

async fn create_mint(banks: &mut BanksClient, payer: &Keypair, mint: &Keypair) {
    let rent = banks.get_rent().await.unwrap();
    let create = system_instruction::create_account(
        &payer.pubkey(),
        &mint.pubkey(),
        rent.minimum_balance(spl_token::state::Mint::LEN),
        spl_token::state::Mint::LEN as u64,
        &spl_token::ID,
    );
    let init = spl_token::instruction::initialize_mint2(
        &spl_token::ID,
        &mint.pubkey(),
        &payer.pubkey(),
        None,
        DECIMALS,
    )
    .unwrap();
    send(banks, &[create, init], payer, &[payer, mint]).await;
}

async fn create_ata(
    banks: &mut BanksClient,
    payer: &Keypair,
    owner: &Pubkey,
    mint: &Pubkey,
) -> Pubkey {
    let ixn = create_associated_token_account(&payer.pubkey(), owner, mint, &spl_token::ID);
    send(banks, &[ixn], payer, &[payer]).await;
    get_associated_token_address(owner, mint)
}

async fn mint_to(
    banks: &mut BanksClient,
    payer: &Keypair,
    mint: &Pubkey,
    ata: &Pubkey,
    amount: u64,
) {
    let ixn =
        spl_token::instruction::mint_to(&spl_token::ID, mint, ata, &payer.pubkey(), &[], amount)
            .unwrap();
    send(banks, &[ixn], payer, &[payer]).await;
}

async fn token_balance(banks: &mut BanksClient, ata: &Pubkey) -> u64 {
    let acc = banks.get_account(*ata).await.unwrap().unwrap();
    spl_token::state::Account::unpack(&acc.data).unwrap().amount
}

/// Read a `u128` pool field past the 8-byte discriminator, by byte offset
/// (avoids casting the unaligned zero-copy buffer). liquidity=0, sqrt_price=16.
async fn pool_u128(banks: &mut BanksClient, pool: &Pubkey, field_offset: usize) -> u128 {
    let acc = banks.get_account(*pool).await.unwrap().unwrap();
    let start = 8 + field_offset;
    u128::from_le_bytes(acc.data[start..start + 16].try_into().unwrap())
}

fn ix(accounts: impl ToAccountMetas, data: impl InstructionData) -> Instruction {
    Instruction {
        program_id: zenith_amm::ID,
        accounts: accounts.to_account_metas(None),
        data: data.data(),
    }
}

#[tokio::test]
async fn full_m1_lifecycle() {
    // Point the harness at the freshly built .so.
    std::env::set_var(
        "SBF_OUT_DIR",
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/deploy"),
    );
    let pt = ProgramTest::new("zenith_amm", zenith_amm::ID, None);
    let (mut banks, payer, _bh) = pt.start().await;

    // Two SPL mints, ordered so mint_a < mint_b (initialize_pool requires it).
    let (mint_a_kp, mint_b_kp) = {
        let (mut x, mut y) = (Keypair::new(), Keypair::new());
        if x.pubkey() > y.pubkey() {
            std::mem::swap(&mut x, &mut y);
        }
        (x, y)
    };
    create_mint(&mut banks, &payer, &mint_a_kp).await;
    create_mint(&mut banks, &payer, &mint_b_kp).await;
    let mint_a = mint_a_kp.pubkey();
    let mint_b = mint_b_kp.pubkey();

    let user_a = create_ata(&mut banks, &payer, &payer.pubkey(), &mint_a).await;
    let user_b = create_ata(&mut banks, &payer, &payer.pubkey(), &mint_b).await;
    let supply = 1_000_000_000_000_000u64; // 1e15
    mint_to(&mut banks, &payer, &mint_a, &user_a, supply).await;
    mint_to(&mut banks, &payer, &mint_b, &user_b, supply).await;

    let (config, _) = zenith_amm::pda::config_pda(0);
    let (pool, _) = zenith_amm::pda::pool_pda(&config, &mint_a, &mint_b);
    let (pool_authority, _) = zenith_amm::pda::pool_authority_pda(&pool);
    let (vault_a, _) = zenith_amm::pda::vault_pda(&pool, &mint_a);
    let (vault_b, _) = zenith_amm::pda::vault_pda(&pool, &mint_b);

    // Band: sqrt in [1, 4] (price 1..16), current sqrt-price 2 (price 4).
    let sqrt_min = Q64;
    let sqrt_max = 4 * Q64;
    let sqrt_price = 2 * Q64;

    // 1) create_config — 0.3% base fee, protocol takes 10% of the fee.
    send(
        &mut banks,
        &[ix(
            zenith_amm::accounts::CreateConfig {
                admin: payer.pubkey(),
                config,
                system_program: solana_sdk::system_program::ID,
            },
            zenith_amm::instruction::CreateConfig {
                index: 0,
                fee_authority: payer.pubkey(),
                sqrt_min_price: sqrt_min,
                sqrt_max_price: sqrt_max,
                base_fee_bps: 30,
                protocol_fee_bps: 1_000,
                // Constant-mode fee scheduler (flat 0.3%); decay params unused.
                fee_scheduler_mode: 0,
                cliff_fee_bps: 0,
                reduction_factor: 0,
                fee_period: 0,
                max_fee_steps: 0,
                // Dynamic (volatility) fee disabled.
                variable_fee_control: 0,
                max_volatility_accumulator: 0,
                filter_period: 0,
                decay_period: 0,
                volatility_reduction_factor: 0,
                max_dynamic_fee_bps: 0,
            },
        )],
        &payer,
        &[&payer],
    )
    .await;

    // 2) initialize_pool — opens position 1 with liquidity L1.
    let l1 = 1_000_000_000_000u128; // 1e12
    let nft1 = Keypair::new();
    let nft1_account = get_associated_token_address(&payer.pubkey(), &nft1.pubkey());
    let (position1, _) = zenith_amm::pda::position_pda(&nft1.pubkey());
    send(
        &mut banks,
        &[ix(
            zenith_amm::accounts::InitializePool {
                creator: payer.pubkey(),
                config,
                token_a_mint: mint_a,
                token_b_mint: mint_b,
                pool,
                pool_authority,
                token_a_vault: vault_a,
                token_b_vault: vault_b,
                creator_token_a: user_a,
                creator_token_b: user_b,
                position_nft_mint: nft1.pubkey(),
                position_nft_account: nft1_account,
                position: position1,
                token_program: spl_token::ID,
                associated_token_program: spl_associated_token_account::ID,
                system_program: solana_sdk::system_program::ID,
            },
            zenith_amm::instruction::InitializePool {
                sqrt_price,
                liquidity: l1,
                token_a_max: u64::MAX,
                token_b_max: u64::MAX,
            },
        )],
        &payer,
        &[&payer, &nft1],
    )
    .await;
    assert_eq!(pool_u128(&mut banks, &pool, 16).await, sqrt_price);
    assert_eq!(pool_u128(&mut banks, &pool, 0).await, l1);

    // 3) create_position (position 2) + add_liquidity L2 = 2 * L1.
    let nft2 = Keypair::new();
    let nft2_account = get_associated_token_address(&payer.pubkey(), &nft2.pubkey());
    let (position2, _) = zenith_amm::pda::position_pda(&nft2.pubkey());
    send(
        &mut banks,
        &[ix(
            zenith_amm::accounts::CreatePosition {
                creator: payer.pubkey(),
                pool,
                pool_authority,
                position_nft_mint: nft2.pubkey(),
                position_nft_account: nft2_account,
                position: position2,
                token_program: spl_token::ID,
                associated_token_program: spl_associated_token_account::ID,
                system_program: solana_sdk::system_program::ID,
            },
            zenith_amm::instruction::CreatePosition {},
        )],
        &payer,
        &[&payer, &nft2],
    )
    .await;

    let l2 = 2_000_000_000_000u128; // 2e12
    let modify_accounts =
        |nft_account: Pubkey, position: Pubkey| zenith_amm::accounts::ModifyLiquidity {
            owner: payer.pubkey(),
            pool,
            position,
            position_nft_account: nft_account,
            pool_authority,
            token_a_vault: vault_a,
            token_b_vault: vault_b,
            user_token_a: user_a,
            user_token_b: user_b,
            token_program: spl_token::ID,
        };
    send(
        &mut banks,
        &[ix(
            modify_accounts(nft2_account, position2),
            zenith_amm::instruction::AddLiquidity {
                liquidity_delta: l2,
                token_a_max: u64::MAX,
                token_b_max: u64::MAX,
            },
        )],
        &payer,
        &[&payer],
    )
    .await;
    assert_eq!(pool_u128(&mut banks, &pool, 0).await, l1 + l2);

    // 4) swaps: BToA ExactIn (fee in B), then AToB ExactOut (fee in A).
    let swap_accounts = || zenith_amm::accounts::Swap {
        owner: payer.pubkey(),
        pool,
        config,
        pool_authority,
        token_a_vault: vault_a,
        token_b_vault: vault_b,
        user_token_a: user_a,
        user_token_b: user_b,
        token_program: spl_token::ID,
    };
    send(
        &mut banks,
        &[ix(
            swap_accounts(),
            zenith_amm::instruction::Swap {
                direction: SwapDirection::BToA,
                mode: SwapMode::ExactIn,
                amount: 50_000_000,
                other_amount_threshold: 0,
            },
        )],
        &payer,
        &[&payer],
    )
    .await;
    send(
        &mut banks,
        &[ix(
            swap_accounts(),
            zenith_amm::instruction::Swap {
                direction: SwapDirection::AToB,
                mode: SwapMode::ExactOut,
                amount: 20_000_000,
                other_amount_threshold: u64::MAX,
            },
        )],
        &payer,
        &[&payer],
    )
    .await;
    let price_after = pool_u128(&mut banks, &pool, 16).await;
    assert!(price_after >= sqrt_min && price_after <= sqrt_max);

    // 5) claim fees on both positions; balance deltas reveal each payout.
    let claim_accounts =
        |nft_account: Pubkey, position: Pubkey| zenith_amm::accounts::ClaimPositionFee {
            owner: payer.pubkey(),
            pool,
            position,
            position_nft_account: nft_account,
            pool_authority,
            token_a_vault: vault_a,
            token_b_vault: vault_b,
            owner_token_a: user_a,
            owner_token_b: user_b,
            token_program: spl_token::ID,
        };
    let (a0, b0) = (
        token_balance(&mut banks, &user_a).await,
        token_balance(&mut banks, &user_b).await,
    );
    send(
        &mut banks,
        &[ix(
            claim_accounts(nft1_account, position1),
            zenith_amm::instruction::ClaimPositionFee {},
        )],
        &payer,
        &[&payer],
    )
    .await;
    let (a1, b1) = (
        token_balance(&mut banks, &user_a).await,
        token_balance(&mut banks, &user_b).await,
    );
    send(
        &mut banks,
        &[ix(
            claim_accounts(nft2_account, position2),
            zenith_amm::instruction::ClaimPositionFee {},
        )],
        &payer,
        &[&payer],
    )
    .await;
    let (a2, b2) = (
        token_balance(&mut banks, &user_a).await,
        token_balance(&mut banks, &user_b).await,
    );

    let (p1_a, p1_b) = (a1 - a0, b1 - b0);
    let (p2_a, p2_b) = (a2 - a1, b2 - b1);
    assert!(p1_a > 0 && p1_b > 0, "position 1 fees: a={p1_a} b={p1_b}");
    assert!(p2_a > 0 && p2_b > 0, "position 2 fees: a={p2_a} b={p2_b}");
    // L2 = 2 * L1 → position 2 earns ~2x position 1 (within rounding).
    assert!(
        (p2_b as i128 - 2 * p1_b as i128).abs() <= 2,
        "B split off: {p1_b} vs {p2_b}"
    );
    assert!(
        (p2_a as i128 - 2 * p1_a as i128).abs() <= 2,
        "A split off: {p1_a} vs {p2_a}"
    );

    // 6) remove all liquidity from both positions.
    for (nft_account, position) in [(nft2_account, position2), (nft1_account, position1)] {
        send(
            &mut banks,
            &[ix(
                modify_accounts(nft_account, position),
                zenith_amm::instruction::RemoveAllLiquidity {
                    token_a_min: 0,
                    token_b_min: 0,
                },
            )],
            &payer,
            &[&payer],
        )
        .await;
    }
    assert_eq!(pool_u128(&mut banks, &pool, 0).await, 0);

    // 7) close both positions (now empty); rent returns to the owner.
    for (nft_mint, nft_account, position) in [
        (nft2.pubkey(), nft2_account, position2),
        (nft1.pubkey(), nft1_account, position1),
    ] {
        send(
            &mut banks,
            &[ix(
                zenith_amm::accounts::ClosePosition {
                    owner: payer.pubkey(),
                    pool,
                    position,
                    position_nft_mint: nft_mint,
                    position_nft_account: nft_account,
                    token_program: spl_token::ID,
                },
                zenith_amm::instruction::ClosePosition {},
            )],
            &payer,
            &[&payer],
        )
        .await;
        assert!(banks.get_account(position).await.unwrap().is_none());
    }
}
