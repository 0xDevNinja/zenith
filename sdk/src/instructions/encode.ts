import type { PublicKey } from "@solana/web3.js";

/// Anchor instruction discriminators: `sha256("global:<snake_name>")[..8]`.
/// The committed IDL carries none, so they are precomputed here (stable per
/// instruction name; verified in the test suite).
export const INSTRUCTION_DISCRIMINATORS = {
  createConfig: [201, 207, 243, 114, 75, 111, 47, 189],
  initializePool: [95, 180, 10, 172, 84, 174, 232, 40],
  createPosition: [48, 215, 197, 153, 96, 203, 180, 133],
  addLiquidity: [181, 157, 89, 67, 143, 182, 52, 72],
  removeLiquidity: [80, 85, 209, 72, 24, 206, 177, 108],
  removeAllLiquidity: [10, 51, 61, 35, 112, 105, 24, 85],
  swap: [248, 198, 158, 145, 225, 117, 135, 200],
  claimPositionFee: [180, 38, 154, 17, 133, 33, 162, 211],
  claimProtocolFee: [165, 228, 133, 48, 99, 249, 255, 33],
  claimPartnerFee: [97, 206, 39, 105, 94, 94, 126, 148],
  setPositionCompounding: [246, 189, 188, 25, 252, 30, 252, 136],
  closePosition: [123, 134, 81, 0, 49, 68, 98, 98],
} as const;

export type InstructionName = keyof typeof INSTRUCTION_DISCRIMINATORS;

const MASK64 = (1n << 64n) - 1n;

/// Little-endian byte writer mirroring borsh scalar encoding — the inverse of
/// the decode `Reader`. Used to lay out instruction arguments.
export class Writer {
  private readonly parts: Buffer[] = [];

  u8(v: number): this {
    const b = Buffer.alloc(1);
    b.writeUInt8(v);
    this.parts.push(b);
    return this;
  }
  u16(v: number): this {
    const b = Buffer.alloc(2);
    b.writeUInt16LE(v);
    this.parts.push(b);
    return this;
  }
  u32(v: number): this {
    const b = Buffer.alloc(4);
    b.writeUInt32LE(v);
    this.parts.push(b);
    return this;
  }
  u64(v: bigint): this {
    const b = Buffer.alloc(8);
    b.writeBigUInt64LE(v);
    this.parts.push(b);
    return this;
  }
  u128(v: bigint): this {
    const b = Buffer.alloc(16);
    b.writeBigUInt64LE(v & MASK64, 0);
    b.writeBigUInt64LE(v >> 64n, 8);
    this.parts.push(b);
    return this;
  }
  i32(v: number): this {
    const b = Buffer.alloc(4);
    b.writeInt32LE(v);
    this.parts.push(b);
    return this;
  }
  i64(v: bigint): this {
    const b = Buffer.alloc(8);
    b.writeBigInt64LE(v);
    this.parts.push(b);
    return this;
  }
  bool(v: boolean): this {
    return this.u8(v ? 1 : 0);
  }
  pubkey(pk: PublicKey): this {
    this.parts.push(Buffer.from(pk.toBuffer()));
    return this;
  }
  bytes(b: Uint8Array | number[]): this {
    this.parts.push(Buffer.from(b));
    return this;
  }
  build(): Buffer {
    return Buffer.concat(this.parts);
  }
}

/// Build instruction data: the 8-byte discriminator followed by borsh args.
export function ixData(name: InstructionName, writeArgs?: (w: Writer) => void): Buffer {
  const w = new Writer();
  w.bytes([...INSTRUCTION_DISCRIMINATORS[name]]);
  writeArgs?.(w);
  return w.build();
}
