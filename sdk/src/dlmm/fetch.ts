import type { PublicKey } from "@solana/web3.js";

import type { ZenithConnection } from "../connection.js";
import {
  type BinArray,
  type LbPair,
  type Oracle,
  type Position,
  decodeBinArray,
  decodeLbPair,
  decodeOracle,
  decodePosition,
} from "./accounts.js";

/// Fetch and decode an `LbPair`, or `null` if it does not exist. Throws if the
/// data is not a valid `LbPair` (wrong discriminator/size).
export async function fetchLbPair(
  conn: ZenithConnection,
  address: PublicKey,
): Promise<LbPair | null> {
  const info = await conn.getAccountInfo(address);
  return info ? decodeLbPair(info.data) : null;
}

/// Fetch and decode a `BinArray`, or `null` if it does not exist.
export async function fetchBinArray(
  conn: ZenithConnection,
  address: PublicKey,
): Promise<BinArray | null> {
  const info = await conn.getAccountInfo(address);
  return info ? decodeBinArray(info.data) : null;
}

/// Fetch and decode a `Position`, or `null` if it does not exist.
export async function fetchDlmmPosition(
  conn: ZenithConnection,
  address: PublicKey,
): Promise<Position | null> {
  const info = await conn.getAccountInfo(address);
  return info ? decodePosition(info.data) : null;
}

/// Fetch and decode an `Oracle`, or `null` if it does not exist.
export async function fetchOracle(
  conn: ZenithConnection,
  address: PublicKey,
): Promise<Oracle | null> {
  const info = await conn.getAccountInfo(address);
  return info ? decodeOracle(info.data) : null;
}
