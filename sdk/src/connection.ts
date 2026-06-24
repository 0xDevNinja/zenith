import {
  type AccountInfo,
  type Commitment,
  Connection,
  type PublicKey,
} from "@solana/web3.js";

export interface ZenithConnectionOptions {
  /// Default commitment for reads (defaults to "confirmed").
  commitment?: Commitment;
  /// Max retries for transient RPC failures (defaults to 3).
  maxRetries?: number;
}

/// Thin wrapper over web3.js `Connection` with a default commitment and a
/// bounded exponential-backoff retry for flaky RPC endpoints.
export class ZenithConnection {
  readonly connection: Connection;
  readonly commitment: Commitment;
  readonly maxRetries: number;

  constructor(endpoint: string | Connection, opts: ZenithConnectionOptions = {}) {
    this.commitment = opts.commitment ?? "confirmed";
    this.maxRetries = opts.maxRetries ?? 3;
    this.connection =
      typeof endpoint === "string" ? new Connection(endpoint, this.commitment) : endpoint;
  }

  /// Run an RPC call, retrying on failure with exponential backoff.
  async withRetry<T>(fn: (c: Connection) => Promise<T>): Promise<T> {
    let lastError: unknown;
    for (let attempt = 0; attempt <= this.maxRetries; attempt++) {
      try {
        return await fn(this.connection);
      } catch (err) {
        lastError = err;
        if (attempt < this.maxRetries) {
          await new Promise((r) => setTimeout(r, 100 * 2 ** attempt));
        }
      }
    }
    throw lastError;
  }

  /// Fetch raw account data (or null) with retry.
  getAccountInfo(pubkey: PublicKey): Promise<AccountInfo<Buffer> | null> {
    return this.withRetry((c) => c.getAccountInfo(pubkey, this.commitment));
  }
}
