import type { PublicKey } from "@solana/web3.js";

import type { ZenithConnection } from "../connection.js";
import { type Pool, decodePool } from "./accounts.js";

/// Fetch and decode a constant-product `Pool`, or `null` if it does not exist.
/// Throws if the data is not a valid `Pool` (wrong discriminator/size).
export async function fetchPool(
  conn: ZenithConnection,
  address: PublicKey,
): Promise<Pool | null> {
  const info = await conn.getAccountInfo(address);
  return info ? decodePool(info.data) : null;
}
