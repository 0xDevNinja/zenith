import { PublicKey } from "@solana/web3.js";

/// Sequential little-endian byte reader.
///
/// Both Anchor's borsh accounts (Config, Position) and the zero-copy `Pool`
/// store fields back-to-back in declaration order with no implicit padding
/// (the zero-copy struct is `Pod`: ordered by descending alignment with
/// explicit reserved/padding arrays). So a single forward cursor with explicit
/// `skip` over reserved regions decodes every account correctly.
export class Reader {
  private readonly view: DataView;
  private offset: number;

  constructor(data: Uint8Array, start = 0) {
    this.view = new DataView(data.buffer, data.byteOffset, data.byteLength);
    this.offset = start;
  }

  /// Current cursor position (bytes from the start of the buffer).
  get position(): number {
    return this.offset;
  }

  /// Advance the cursor without reading (used for reserved/padding regions).
  skip(bytes: number): this {
    this.offset += bytes;
    return this;
  }

  u8(): number {
    const v = this.view.getUint8(this.offset);
    this.offset += 1;
    return v;
  }

  u16(): number {
    const v = this.view.getUint16(this.offset, true);
    this.offset += 2;
    return v;
  }

  u32(): number {
    const v = this.view.getUint32(this.offset, true);
    this.offset += 4;
    return v;
  }

  u64(): bigint {
    const v = this.view.getBigUint64(this.offset, true);
    this.offset += 8;
    return v;
  }

  u128(): bigint {
    const lo = this.view.getBigUint64(this.offset, true);
    const hi = this.view.getBigUint64(this.offset + 8, true);
    this.offset += 16;
    return (hi << 64n) | lo;
  }

  i32(): number {
    const v = this.view.getInt32(this.offset, true);
    this.offset += 4;
    return v;
  }

  i64(): bigint {
    const v = this.view.getBigInt64(this.offset, true);
    this.offset += 8;
    return v;
  }

  i128(): bigint {
    const lo = this.view.getBigUint64(this.offset, true);
    const hi = this.view.getBigUint64(this.offset + 8, true);
    this.offset += 16;
    const unsigned = (hi << 64n) | lo;
    // Two's-complement: values with the top bit set are negative.
    return unsigned >= 1n << 127n ? unsigned - (1n << 128n) : unsigned;
  }

  pubkey(): PublicKey {
    // Bounds-check against the logical view length. Unlike the DataView
    // numeric reads (which are bounds-checked), constructing a Uint8Array over
    // the raw backing buffer would otherwise read adjacent (pooled) memory on a
    // short account instead of throwing.
    if (this.offset + 32 > this.view.byteLength) {
      throw new RangeError(`pubkey read out of bounds at offset ${this.offset}`);
    }
    const bytes = new Uint8Array(this.view.buffer, this.view.byteOffset + this.offset, 32);
    this.offset += 32;
    return new PublicKey(bytes);
  }
}
