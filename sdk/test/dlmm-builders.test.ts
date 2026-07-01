import { PublicKey } from "@solana/web3.js";
import { describe, expect, it } from "vitest";

import { dlmm } from "../src/index.js";

const pk = (b: number) => new PublicKey(Uint8Array.from(Array(32).fill(b)));

// distinct dummy accounts
const creator = pk(1);
const lbPair = pk(2);
const pairAuthority = pk(3);
const reserveX = pk(4);
const reserveY = pk(5);
const userX = pk(6);
const userY = pk(7);
const oracle = pk(8);
const arr0 = pk(9);
const arrNeg = pk(10);

function u64le(buf: Buffer, off: number): bigint {
  return buf.readBigUInt64LE(off);
}

describe("dlmm instruction builders", () => {
  it("swap: discriminator, args, and account order/flags", () => {
    const ix = dlmm.buildSwap({
      trader: creator,
      lbPair,
      pairAuthority,
      reserveX,
      reserveY,
      userTokenX: userX,
      userTokenY: userY,
      binArrays: [arr0, arrNeg],
      oracle,
      direction: dlmm.Direction.YtoX,
      mode: dlmm.SwapMode.ExactIn,
      amount: 1234n,
      otherAmountThreshold: 999n,
    });
    const d = Buffer.from(ix.data);
    expect([...d.subarray(0, 8)]).toEqual(dlmm.DLMM_INSTRUCTION_DISCRIMINATORS.swap);
    expect(d[8]).toBe(1); // direction YtoX
    expect(d[9]).toBe(0); // mode ExactIn
    expect(u64le(d, 10)).toBe(1234n);
    expect(u64le(d, 18)).toBe(999n);

    // 8 named accounts + oracle + 2 bin arrays
    expect(ix.keys.length).toBe(11);

    // trader is the only signer; lb_pair + reserves + user + oracle + bin arrays writable
    expect(ix.keys[0].isSigner).toBe(true);
    expect(ix.keys[0].isWritable).toBe(false);
    expect(ix.keys[1].pubkey.equals(lbPair)).toBe(true);
    expect(ix.keys[1].isWritable).toBe(true);
    expect(ix.keys[2].pubkey.equals(pairAuthority)).toBe(true);
    expect(ix.keys[2].isWritable).toBe(false);
    // oracle is at index 8 (after 8 named: trader,lbPair,auth,rX,rY,uX,uY,tokenProgram)
    expect(ix.keys[8].pubkey.equals(oracle)).toBe(true);
    expect(ix.keys[8].isWritable).toBe(true);
    // remaining bin arrays, writable, non-signer
    expect(ix.keys[9].pubkey.equals(arr0)).toBe(true);
    expect(ix.keys[9].isWritable).toBe(true);
    expect(ix.keys[9].isSigner).toBe(false);
    expect(ix.keys[10].pubkey.equals(arrNeg)).toBe(true);
  });

  it("initialize_lb_pair: encodes a negative active bin (i32) and fee params", () => {
    const ix = dlmm.buildInitializeLbPair({
      creator,
      tokenXMint: pk(20),
      tokenYMint: pk(21),
      lbPair,
      pairAuthority,
      reserveX,
      reserveY,
      binStep: 25,
      activeBinId: -7,
      baseFeeBps: 30,
      protocolFeeRate: 2000,
    });
    const d = Buffer.from(ix.data);
    expect([...d.subarray(0, 8)]).toEqual(dlmm.DLMM_INSTRUCTION_DISCRIMINATORS.initializeLbPair);
    expect(d.readUInt16LE(8)).toBe(25); // bin_step
    expect(d.readInt32LE(10)).toBe(-7); // active_bin_id (two's complement)
    expect(d.readUInt16LE(14)).toBe(30); // base_fee_bps
    expect(d.readUInt16LE(16)).toBe(2000); // protocol_fee_rate
    // creator is signer+writable; lb_pair + reserves writable
    expect(ix.keys[0].isSigner && ix.keys[0].isWritable).toBe(true);
  });

  it("initialize_position: i32 lower bin + u32 width", () => {
    const ix = dlmm.buildInitializePosition({
      owner: creator,
      base: pk(30),
      lbPair,
      position: pk(31),
      lowerBinId: -10,
      width: 11,
    });
    const d = Buffer.from(ix.data);
    expect(d.readInt32LE(8)).toBe(-10);
    expect(d.readUInt32LE(12)).toBe(11);
    // owner + base are both signers
    expect(ix.keys[0].isSigner).toBe(true);
    expect(ix.keys[1].isSigner).toBe(true);
    expect(ix.keys[1].isWritable).toBe(false);
  });

  it("add_liquidity_by_strategy: u64/u8/u128/i32/u32 arg layout", () => {
    const ix = dlmm.buildAddLiquidityByStrategy({
      owner: creator,
      lbPair,
      position: pk(31),
      binArray: arr0,
      reserveX,
      reserveY,
      userTokenX: userX,
      userTokenY: userY,
      amountX: 100n,
      amountY: 200n,
      strategy: 2,
      expectedActiveBinId: 0,
    });
    const d = Buffer.from(ix.data);
    expect(u64le(d, 8)).toBe(100n); // amount_x
    expect(u64le(d, 16)).toBe(200n); // amount_y
    expect(d[24]).toBe(2); // strategy BidAsk
    // min_liquidity_shares u128 at 25..41, expected_active i32 at 41
    expect(d.readInt32LE(41)).toBe(0);
    // owner is signer but NOT writable (matches the program's AddLiquidity)
    expect(ix.keys[0].isSigner).toBe(true);
    expect(ix.keys[0].isWritable).toBe(false);
  });

  it("claim_fee: bin_array is read-only, reserves writable", () => {
    const ix = dlmm.buildClaimFee({
      owner: creator,
      lbPair,
      position: pk(31),
      binArray: arr0,
      pairAuthority,
      reserveX,
      reserveY,
      userTokenX: userX,
      userTokenY: userY,
    });
    // keys: owner, lbPair, position(w), binArray(ro), pairAuth(ro), rX(w), rY(w), uX(w), uY(w), tokenProg
    expect(ix.keys[3].pubkey.equals(arr0)).toBe(true);
    expect(ix.keys[3].isWritable).toBe(false); // claim settles position checkpoints, not the bin
    expect(ix.keys[2].isWritable).toBe(true); // position
  });
});
