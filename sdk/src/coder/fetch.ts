import type { PublicKey } from "@solana/web3.js";
import type { ZenithConnection } from "../connection.js";
import {
  type Config,
  decodeConfig,
  decodePool,
  decodePosition,
  type Pool,
  type Position,
} from "./accounts.js";

/// Fetch and decode a `Pool` account. Returns `null` if the account does not
/// exist; throws if the data is not a valid `Pool` (wrong discriminator/size).
export async function fetchPool(
  conn: ZenithConnection,
  address: PublicKey,
): Promise<Pool | null> {
  const info = await conn.getAccountInfo(address);
  return info ? decodePool(info.data) : null;
}

/// Fetch and decode a `Position` account, or `null` if it does not exist.
export async function fetchPosition(
  conn: ZenithConnection,
  address: PublicKey,
): Promise<Position | null> {
  const info = await conn.getAccountInfo(address);
  return info ? decodePosition(info.data) : null;
}

/// Fetch and decode a `Config` account, or `null` if it does not exist.
export async function fetchConfig(
  conn: ZenithConnection,
  address: PublicKey,
): Promise<Config | null> {
  const info = await conn.getAccountInfo(address);
  return info ? decodeConfig(info.data) : null;
}
