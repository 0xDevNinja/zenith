//! Seed a live zenith-dlmm liquidity-book pair on devnet.
//!
//! Reuses the program's own `accounts` / `instruction` / `pda` types (the exact
//! encodings the on-chain lifecycle test is proven against) but sends the
//! transactions over RPC with the local CLI wallet instead of a BanksClient.
//!
//! Flow: create two SPL mints (tX/tY) → initialize the pair (base + dynamic
//! fee) → two bin arrays (0 and -1) → the TWAP oracle → two positions seeded
//! with a Spot deposit on each side of the active bin. Writes the resulting
//! addresses to `app/src/dlmm-devnet.json` for the SDK and frontend to consume.
//!
//! Run:  cargo run -p dlmm-seed --release

use std::{fs, path::PathBuf};

use anchor_lang::{InstructionData, ToAccountMetas};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    instruction::Instruction,
    program_pack::Pack,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    system_instruction, system_program,
    transaction::Transaction,
};
use spl_associated_token_account::{
    get_associated_token_address, instruction::create_associated_token_account,
};

const DECIMALS: u8 = 6;
const BIN_STEP: u16 = 25; // 0.25% per bin
const BASE_FEE_BPS: u16 = 30; // 0.30%
const PROTOCOL_RATE: u16 = 2_000; // protocol takes 20% of each fee
const SPOT: u8 = 0;

// Dynamic (volatility) fee — enabled so the live pair exercises the surcharge.
const VARIABLE_FEE_CONTROL: u32 = 1_000_000;
const MAX_VOLATILITY_ACCUMULATOR: u32 = 100_000;
const FILTER_PERIOD: u32 = 10;
const DECAY_PERIOD: u32 = 100;
const VOLATILITY_REDUCTION_FACTOR: u16 = 5_000; // 50%
const MAX_DYNAMIC_FEE_BPS: u16 = 1_000; // cap surcharge at 10%

fn ix(accounts: impl ToAccountMetas, data: impl InstructionData) -> Instruction {
    Instruction {
        program_id: zenith_dlmm::ID,
        accounts: accounts.to_account_metas(None),
        data: data.data(),
    }
}

fn load_cli_keypair() -> Keypair {
    let path = dirs_home().join(".config/solana/id.json");
    let bytes: Vec<u8> = serde_json::from_slice(&fs::read(&path).expect("read id.json"))
        .expect("parse id.json");
    Keypair::from_bytes(&bytes).expect("keypair from bytes")
}

fn dirs_home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").expect("HOME set"))
}

fn main() {
    let rpc = std::env::var("RPC").unwrap_or_else(|_| "https://api.devnet.solana.com".to_string());
    let client = RpcClient::new_with_commitment(rpc.clone(), CommitmentConfig::confirmed());
    let payer = load_cli_keypair();
    println!("payer    {}", payer.pubkey());
    println!("cluster  {rpc}");
    println!("program  {}", zenith_dlmm::ID);

    let send = |ixs: &[Instruction], signers: &[&Keypair], label: &str| {
        let bh = client.get_latest_blockhash().expect("blockhash");
        let tx = Transaction::new_signed_with_payer(ixs, Some(&payer.pubkey()), signers, bh);
        let sig = client
            .send_and_confirm_transaction(&tx)
            .unwrap_or_else(|e| panic!("{label} failed: {e}"));
        println!("  ✓ {label:<22} {sig}");
    };

    // --- SPL mints, ordered x < y (initialize_lb_pair requires ascending). ---
    let (x_kp, y_kp) = {
        let (mut a, mut b) = (Keypair::new(), Keypair::new());
        if a.pubkey() > b.pubkey() {
            std::mem::swap(&mut a, &mut b);
        }
        (a, b)
    };
    let (mint_x, mint_y) = (x_kp.pubkey(), y_kp.pubkey());
    let mint_rent = client
        .get_minimum_balance_for_rent_exemption(spl_token::state::Mint::LEN)
        .expect("mint rent");
    for kp in [&x_kp, &y_kp] {
        let create = system_instruction::create_account(
            &payer.pubkey(),
            &kp.pubkey(),
            mint_rent,
            spl_token::state::Mint::LEN as u64,
            &spl_token::ID,
        );
        let init = spl_token::instruction::initialize_mint2(
            &spl_token::ID,
            &kp.pubkey(),
            &payer.pubkey(),
            None,
            DECIMALS,
        )
        .unwrap();
        send(&[create, init], &[&payer, kp], "create mint");
    }

    // --- user ATAs + supply ---
    let user_x = get_associated_token_address(&payer.pubkey(), &mint_x);
    let user_y = get_associated_token_address(&payer.pubkey(), &mint_y);
    for mint in [&mint_x, &mint_y] {
        send(
            &[create_associated_token_account(
                &payer.pubkey(),
                &payer.pubkey(),
                mint,
                &spl_token::ID,
            )],
            &[&payer],
            "create ATA",
        );
    }
    let supply = 1_000_000_000_000u64; // 1e12 (1,000,000 tokens at 6 dp)
    for (mint, ata) in [(&mint_x, &user_x), (&mint_y, &user_y)] {
        send(
            &[
                spl_token::instruction::mint_to(&spl_token::ID, mint, ata, &payer.pubkey(), &[], supply)
                    .unwrap(),
            ],
            &[&payer],
            "mint supply",
        );
    }

    // --- PDAs ---
    let (lb_pair, _) = zenith_dlmm::pda::lb_pair_pda(&mint_x, &mint_y, BIN_STEP);
    let (pair_authority, _) = zenith_dlmm::pda::pair_authority_pda(&lb_pair);
    let (reserve_x, _) = zenith_dlmm::pda::reserve_pda(&lb_pair, &mint_x);
    let (reserve_y, _) = zenith_dlmm::pda::reserve_pda(&lb_pair, &mint_y);
    let (bin_array_0, _) = zenith_dlmm::pda::bin_array_pda(&lb_pair, 0);
    let (bin_array_neg1, _) = zenith_dlmm::pda::bin_array_pda(&lb_pair, -1);
    let (oracle, _) = zenith_dlmm::pda::oracle_pda(&lb_pair);

    // --- 1) initialize the pair at active bin 0 ---
    send(
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
                system_program: system_program::ID,
            },
            zenith_dlmm::instruction::InitializeLbPair {
                bin_step: BIN_STEP,
                active_bin_id: 0,
                base_fee_bps: BASE_FEE_BPS,
                protocol_fee_rate: PROTOCOL_RATE,
                variable_fee_control: VARIABLE_FEE_CONTROL,
                max_volatility_accumulator: MAX_VOLATILITY_ACCUMULATOR,
                filter_period: FILTER_PERIOD,
                decay_period: DECAY_PERIOD,
                volatility_reduction_factor: VOLATILITY_REDUCTION_FACTOR,
                max_dynamic_fee_bps: MAX_DYNAMIC_FEE_BPS,
            },
        )],
        &[&payer],
        "initialize_lb_pair",
    );

    // --- 2) two bin arrays: index 0 (bins 0..69) and index -1 (bins -70..-1) ---
    for index in [0i64, -1] {
        let (bin_array, _) = zenith_dlmm::pda::bin_array_pda(&lb_pair, index);
        send(
            &[ix(
                zenith_dlmm::accounts::InitializeBinArray {
                    payer: payer.pubkey(),
                    lb_pair,
                    bin_array,
                    system_program: system_program::ID,
                },
                zenith_dlmm::instruction::InitializeBinArray { index },
            )],
            &[&payer],
            "initialize_bin_array",
        );
    }

    // --- 3) TWAP oracle (length 8) ---
    send(
        &[ix(
            zenith_dlmm::accounts::InitializeOracle {
                payer: payer.pubkey(),
                lb_pair,
                oracle,
                system_program: system_program::ID,
            },
            zenith_dlmm::instruction::InitializeOracle { length: 8 },
        )],
        &[&payer],
        "initialize_oracle",
    );

    // helper to add a Spot deposit to a position within one bin array.
    let add_ix = |position: Pubkey, bin_array: Pubkey, amount_x: u64, amount_y: u64| {
        ix(
            zenith_dlmm::accounts::AddLiquidity {
                owner: payer.pubkey(),
                lb_pair,
                position,
                bin_array,
                reserve_x,
                reserve_y,
                user_token_x: user_x,
                user_token_y: user_y,
                token_program: spl_token::ID,
            },
            zenith_dlmm::instruction::AddLiquidityByStrategy {
                amount_x,
                amount_y,
                strategy: SPOT,
                min_liquidity_shares: 0,
                expected_active_bin_id: 0,
                active_id_slippage: 0,
            },
        )
    };

    // --- 4) position A: [0, 10] (array 0) — X across the range + Y at active ---
    let base_a = Keypair::new();
    let (position_a, _) = zenith_dlmm::pda::position_pda(&base_a.pubkey());
    send(
        &[ix(
            zenith_dlmm::accounts::InitializePosition {
                owner: payer.pubkey(),
                base: base_a.pubkey(),
                lb_pair,
                position: position_a,
                system_program: system_program::ID,
            },
            zenith_dlmm::instruction::InitializePosition {
                lower_bin_id: 0,
                width: 11,
            },
        )],
        &[&payer, &base_a],
        "init position A",
    );
    send(
        &[add_ix(position_a, bin_array_0, 220_000_000, 100_000_000)],
        &[&payer],
        "add liquidity A",
    );

    // --- 5) position B: [-10, -1] (array -1) — one-sided Y below the active bin ---
    let base_b = Keypair::new();
    let (position_b, _) = zenith_dlmm::pda::position_pda(&base_b.pubkey());
    send(
        &[ix(
            zenith_dlmm::accounts::InitializePosition {
                owner: payer.pubkey(),
                base: base_b.pubkey(),
                lb_pair,
                position: position_b,
                system_program: system_program::ID,
            },
            zenith_dlmm::instruction::InitializePosition {
                lower_bin_id: -10,
                width: 10,
            },
        )],
        &[&payer, &base_b],
        "init position B",
    );
    send(
        &[add_ix(position_b, bin_array_neg1, 0, 200_000_000)],
        &[&payer],
        "add liquidity B",
    );

    // --- manifest ---
    let manifest = serde_json::json!({
        "cluster": "devnet",
        "programId": zenith_dlmm::ID.to_string(),
        "lbPair": lb_pair.to_string(),
        "pairAuthority": pair_authority.to_string(),
        "oracle": oracle.to_string(),
        "reserveX": reserve_x.to_string(),
        "reserveY": reserve_y.to_string(),
        "binArrays": { "0": bin_array_0.to_string(), "-1": bin_array_neg1.to_string() },
        "binStep": BIN_STEP,
        "activeBinId": 0,
        "baseFeeBps": BASE_FEE_BPS,
        "protocolFeeRate": PROTOCOL_RATE,
        "positions": [position_a.to_string(), position_b.to_string()],
        "tokenX": mint_x.to_string(),
        "tokenY": mint_y.to_string(),
        "mints": {
            mint_x.to_string(): { "symbol": "tBIN", "decimals": DECIMALS },
            mint_y.to_string(): { "symbol": "tUSD", "decimals": DECIMALS },
        },
    });
    let out = dirs_home().join("zenith/app/src/dlmm-devnet.json");
    fs::write(&out, serde_json::to_string_pretty(&manifest).unwrap() + "\n").expect("write manifest");
    println!("\nmanifest → {}", out.display());
    println!("lb_pair  {lb_pair}");
    println!("oracle   {oracle}");
    println!("done — DLMM pair seeded on devnet.");
}
