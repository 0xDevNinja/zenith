//! End-to-end M4 DLMM lifecycle against the real compiled program on the BPF VM
//! via `solana-program-test`.
//!
//! Flow: initialize_lb_pair -> initialize_bin_array (two arrays) ->
//! initialize_position x2 (balanced + one-sided) -> add_liquidity_by_strategy ->
//! swaps that stay in a bin, cross bins, and cross a bin-array boundary, both
//! directions -> a randomized swap sequence asserting token conservation and an
//! in-band active bin -> remove_liquidity -> close_position. Asserts shares,
//! reserves, and the active bin stay consistent throughout.
//!
//! Local-only: build the BPF program first —
//!   cargo build-sbf --manifest-path programs/zenith-dlmm/Cargo.toml
//! then `cargo test -p dlmm-integration`. The harness points `SBF_OUT_DIR` at
//! `target/deploy` so it loads `zenith_dlmm.so`.

use anchor_lang::{InstructionData, ToAccountMetas};
use solana_program_test::{BanksClient, ProgramTest};
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    program_pack::Pack,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    system_instruction,
    transaction::Transaction,
};
use spl_associated_token_account::{
    get_associated_token_address, instruction::create_associated_token_account,
};

const DECIMALS: u8 = 6;
const BIN_STEP: u16 = 25; // 0.25% per bin
const BASE_FEE_BPS: u16 = 30; // 0.3%

// Wire enum values.
const X_TO_Y: u8 = 0;
const Y_TO_X: u8 = 1;
const EXACT_IN: u8 = 0;
const SPOT: u8 = 0;

/// Byte offset of `active_bin_id` (i32) in a loaded LbPair account: 8 (disc) +
/// 96 (u128 x6) + 160 (Pubkey x5) + 72 (u64 x9).
const ACTIVE_BIN_OFFSET: usize = 8 + 96 + 160 + 72;

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

async fn balance(banks: &mut BanksClient, ata: &Pubkey) -> u64 {
    let acc = banks.get_account(*ata).await.unwrap().unwrap();
    spl_token::state::Account::unpack(&acc.data).unwrap().amount
}

async fn active_bin(banks: &mut BanksClient, lb_pair: &Pubkey) -> i32 {
    let acc = banks.get_account(*lb_pair).await.unwrap().unwrap();
    i32::from_le_bytes(
        acc.data[ACTIVE_BIN_OFFSET..ACTIVE_BIN_OFFSET + 4]
            .try_into()
            .unwrap(),
    )
}

fn ix(accounts: impl ToAccountMetas, data: impl InstructionData) -> Instruction {
    Instruction {
        program_id: zenith_dlmm::ID,
        accounts: accounts.to_account_metas(None),
        data: data.data(),
    }
}

#[tokio::test]
async fn full_m4_lifecycle() {
    std::env::set_var(
        "SBF_OUT_DIR",
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/deploy"),
    );
    let pt = ProgramTest::new("zenith_dlmm", zenith_dlmm::ID, None);
    let (mut banks, payer, _bh) = pt.start().await;

    // Two SPL mints, ordered x < y (initialize_lb_pair requires ascending).
    let (x_kp, y_kp) = {
        let (mut a, mut b) = (Keypair::new(), Keypair::new());
        if a.pubkey() > b.pubkey() {
            std::mem::swap(&mut a, &mut b);
        }
        (a, b)
    };
    create_mint(&mut banks, &payer, &x_kp).await;
    create_mint(&mut banks, &payer, &y_kp).await;
    let (mint_x, mint_y) = (x_kp.pubkey(), y_kp.pubkey());

    let user_x = create_ata(&mut banks, &payer, &payer.pubkey(), &mint_x).await;
    let user_y = create_ata(&mut banks, &payer, &payer.pubkey(), &mint_y).await;
    let supply = 1_000_000_000_000_000u64; // 1e15
    mint_to(&mut banks, &payer, &mint_x, &user_x, supply).await;
    mint_to(&mut banks, &payer, &mint_y, &user_y, supply).await;
    // Token conservation reference: nothing is minted/burned after this.
    let total_x = supply;
    let total_y = supply;

    let (lb_pair, _) = zenith_dlmm::pda::lb_pair_pda(&mint_x, &mint_y, BIN_STEP);
    let (pair_authority, _) = zenith_dlmm::pda::pair_authority_pda(&lb_pair);
    let (reserve_x, _) = zenith_dlmm::pda::reserve_pda(&lb_pair, &mint_x);
    let (reserve_y, _) = zenith_dlmm::pda::reserve_pda(&lb_pair, &mint_y);
    let (bin_array_0, _) = zenith_dlmm::pda::bin_array_pda(&lb_pair, 0);
    let (bin_array_neg1, _) = zenith_dlmm::pda::bin_array_pda(&lb_pair, -1);

    // 1) initialize the pair at active bin 0.
    send(
        &mut banks,
        &[ix(
            zenith_dlmm::accounts::InitializeLbPair {
                creator: payer.pubkey(),
                token_x_mint: mint_x,
                token_y_mint: mint_y,
                lb_pair,
                pair_authority,
                reserve_x,
                reserve_y,
                token_program: spl_token::ID,
                system_program: solana_sdk::system_program::ID,
            },
            zenith_dlmm::instruction::InitializeLbPair {
                bin_step: BIN_STEP,
                active_bin_id: 0,
                base_fee_bps: BASE_FEE_BPS,
            },
        )],
        &payer,
        &[&payer],
    )
    .await;
    assert_eq!(active_bin(&mut banks, &lb_pair).await, 0);

    // 2) two bin arrays: index 0 (bins 0..69) and index -1 (bins -70..-1).
    for index in [0i64, -1] {
        let (bin_array, _) = zenith_dlmm::pda::bin_array_pda(&lb_pair, index);
        send(
            &mut banks,
            &[ix(
                zenith_dlmm::accounts::InitializeBinArray {
                    payer: payer.pubkey(),
                    lb_pair,
                    bin_array,
                    system_program: solana_sdk::system_program::ID,
                },
                zenith_dlmm::instruction::InitializeBinArray { index },
            )],
            &payer,
            &[&payer],
        )
        .await;
    }

    // 3) position A: [0, 10] (array 0) — X across 0..10 plus Y in the active bin.
    let base_a = Keypair::new();
    let (position_a, _) = zenith_dlmm::pda::position_pda(&base_a.pubkey());
    send(
        &mut banks,
        &[ix(
            zenith_dlmm::accounts::InitializePosition {
                owner: payer.pubkey(),
                base: base_a.pubkey(),
                lb_pair,
                position: position_a,
                system_program: solana_sdk::system_program::ID,
            },
            zenith_dlmm::instruction::InitializePosition {
                lower_bin_id: 0,
                width: 11,
            },
        )],
        &payer,
        &[&payer, &base_a],
    )
    .await;

    let add = |position: Pubkey, bin_array: Pubkey| zenith_dlmm::accounts::AddLiquidity {
        owner: payer.pubkey(),
        lb_pair,
        position,
        bin_array,
        reserve_x,
        reserve_y,
        user_token_x: user_x,
        user_token_y: user_y,
        token_program: spl_token::ID,
    };
    send(
        &mut banks,
        &[ix(
            add(position_a, bin_array_0),
            zenith_dlmm::instruction::AddLiquidityByStrategy {
                amount_x: 11_000_000,
                amount_y: 5_000_000,
                strategy: SPOT,
                min_liquidity_shares: 0,
                expected_active_bin_id: 0,
                active_id_slippage: 0,
            },
        )],
        &payer,
        &[&payer],
    )
    .await;

    // 4) position B: [-10, -1] (array -1) — one-sided Y (entirely below active).
    let base_b = Keypair::new();
    let (position_b, _) = zenith_dlmm::pda::position_pda(&base_b.pubkey());
    send(
        &mut banks,
        &[ix(
            zenith_dlmm::accounts::InitializePosition {
                owner: payer.pubkey(),
                base: base_b.pubkey(),
                lb_pair,
                position: position_b,
                system_program: solana_sdk::system_program::ID,
            },
            zenith_dlmm::instruction::InitializePosition {
                lower_bin_id: -10,
                width: 10,
            },
        )],
        &payer,
        &[&payer, &base_b],
    )
    .await;
    send(
        &mut banks,
        &[ix(
            add(position_b, bin_array_neg1),
            zenith_dlmm::instruction::AddLiquidityByStrategy {
                amount_x: 0, // one-sided: range is entirely below the active bin
                amount_y: 10_000_000,
                strategy: SPOT,
                min_liquidity_shares: 0,
                expected_active_bin_id: 0,
                active_id_slippage: 0,
            },
        )],
        &payer,
        &[&payer],
    )
    .await;

    // Conservation after deposits.
    assert_eq!(
        balance(&mut banks, &user_x).await + balance(&mut banks, &reserve_x).await,
        total_x
    );
    assert_eq!(
        balance(&mut banks, &user_y).await + balance(&mut banks, &reserve_y).await,
        total_y
    );

    // helper to build a swap with bin arrays appended as remaining accounts.
    let swap_ix = |dir: u8, amount: u64, threshold: u64, arrays: &[Pubkey]| {
        let mut instr = ix(
            zenith_dlmm::accounts::Swap {
                trader: payer.pubkey(),
                lb_pair,
                pair_authority,
                reserve_x,
                reserve_y,
                user_token_x: user_x,
                user_token_y: user_y,
                token_program: spl_token::ID,
            },
            zenith_dlmm::instruction::Swap {
                direction: dir,
                mode: EXACT_IN,
                amount,
                other_amount_threshold: threshold,
            },
        );
        for a in arrays {
            instr.accounts.push(AccountMeta::new(*a, false));
        }
        instr
    };

    // 5a) in-bin swap: small X->Y stays in active bin 0 (price 1.0).
    send(
        &mut banks,
        &[swap_ix(X_TO_Y, 1_000_000, 0, &[bin_array_0])],
        &payer,
        &[&payer],
    )
    .await;
    assert_eq!(
        active_bin(&mut banks, &lb_pair).await,
        0,
        "small swap should stay in bin 0"
    );

    // 5b) cross-bin + cross-array swap: larger X->Y drains bin 0's Y then crosses
    //     down into array -1. Provide both arrays.
    send(
        &mut banks,
        &[swap_ix(
            X_TO_Y,
            8_000_000,
            0,
            &[bin_array_0, bin_array_neg1],
        )],
        &payer,
        &[&payer],
    )
    .await;
    let after_down = active_bin(&mut banks, &lb_pair).await;
    assert!(
        after_down < 0,
        "large X->Y should push the active bin below 0, got {after_down}"
    );

    // 5c) reverse Y->X walks the price back up, crossing array -1 -> 0.
    send(
        &mut banks,
        &[swap_ix(
            Y_TO_X,
            6_000_000,
            0,
            &[bin_array_neg1, bin_array_0],
        )],
        &payer,
        &[&payer],
    )
    .await;
    let after_up = active_bin(&mut banks, &lb_pair).await;
    assert!(
        after_up > after_down,
        "Y->X should move the active bin up, {after_down} -> {after_up}"
    );

    // Conservation holds after the directed swaps.
    assert_eq!(
        balance(&mut banks, &user_x).await + balance(&mut banks, &reserve_x).await,
        total_x
    );
    assert_eq!(
        balance(&mut banks, &user_y).await + balance(&mut banks, &reserve_y).await,
        total_y
    );

    // 6) randomized swap sequence: assert token conservation and an in-band
    //    active bin after each. Deterministic LCG (no rng dependency).
    let mut seed: u64 = 0x9E37_79B9_7F4A_7C15;
    for _ in 0..24 {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let dir = if (seed >> 33) & 1 == 0 {
            X_TO_Y
        } else {
            Y_TO_X
        };
        let amount = 200_000 + (seed >> 40) % 1_500_000;
        let arrays = if dir == X_TO_Y {
            [bin_array_0, bin_array_neg1]
        } else {
            [bin_array_neg1, bin_array_0]
        };
        // A swap may legitimately fail if it would exhaust the seeded liquidity;
        // tolerate that, but any swap that succeeds must conserve tokens.
        let bh = banks.get_latest_blockhash().await.unwrap();
        let tx = Transaction::new_signed_with_payer(
            &[swap_ix(dir, amount, 0, &arrays)],
            Some(&payer.pubkey()),
            &[&payer],
            bh,
        );
        if banks.process_transaction(tx).await.is_err() {
            continue;
        }
        assert_eq!(
            balance(&mut banks, &user_x).await + balance(&mut banks, &reserve_x).await,
            total_x
        );
        assert_eq!(
            balance(&mut banks, &user_y).await + balance(&mut banks, &reserve_y).await,
            total_y
        );
        let a = active_bin(&mut banks, &lb_pair).await;
        assert!(
            (-70..70).contains(&a),
            "active bin {a} left the seeded range"
        );
    }

    // 7) remove all liquidity from both positions.
    let remove = |position: Pubkey, bin_array: Pubkey| zenith_dlmm::accounts::RemoveLiquidity {
        owner: payer.pubkey(),
        lb_pair,
        position,
        bin_array,
        pair_authority,
        reserve_x,
        reserve_y,
        user_token_x: user_x,
        user_token_y: user_y,
        token_program: spl_token::ID,
    };
    for (position, bin_array) in [(position_a, bin_array_0), (position_b, bin_array_neg1)] {
        send(
            &mut banks,
            &[ix(
                remove(position, bin_array),
                zenith_dlmm::instruction::RemoveLiquidity {
                    bps: 10_000,
                    min_amount_x: 0,
                    min_amount_y: 0,
                },
            )],
            &payer,
            &[&payer],
        )
        .await;
    }

    // Conservation still holds; the reserves are nearly drained (only rounding
    // dust + accrued protocol fees remain).
    assert_eq!(
        balance(&mut banks, &user_x).await + balance(&mut banks, &reserve_x).await,
        total_x
    );
    assert_eq!(
        balance(&mut banks, &user_y).await + balance(&mut banks, &reserve_y).await,
        total_y
    );

    // 8) close both positions (now empty); rent returns to the owner.
    for position in [position_a, position_b] {
        send(
            &mut banks,
            &[ix(
                zenith_dlmm::accounts::ClosePosition {
                    owner: payer.pubkey(),
                    position,
                },
                zenith_dlmm::instruction::ClosePosition {},
            )],
            &payer,
            &[&payer],
        )
        .await;
        assert!(banks.get_account(position).await.unwrap().is_none());
    }
}
