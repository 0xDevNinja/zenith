import { useCallback, useEffect, useState } from "react";
import { PublicKey } from "@solana/web3.js";
import { getAssociatedTokenAddressSync } from "@solana/spl-token";
import { useConnection, useWallet } from "@solana/wallet-adapter-react";

interface TokenBalance {
  /// Raw base units, or null while disconnected/loading.
  raw: bigint | null;
  refetch: () => void;
}

// Wallet's SPL balance for a mint. Missing token account reads as 0 (not an
// error) — a fresh wallet simply hasn't received the token yet.
export function useTokenBalance(mint: PublicKey | null): TokenBalance {
  const { connection } = useConnection();
  const { publicKey } = useWallet();
  const [raw, setRaw] = useState<bigint | null>(null);
  const [nonce, setNonce] = useState(0);

  const refetch = useCallback(() => setNonce((n) => n + 1), []);

  useEffect(() => {
    if (!publicKey || !mint) {
      setRaw(null);
      return;
    }
    let active = true;
    const ata = getAssociatedTokenAddressSync(mint, publicKey, true);
    (async () => {
      try {
        const bal = await connection.getTokenAccountBalance(ata);
        if (active) setRaw(BigInt(bal.value.amount));
      } catch {
        // No account yet → zero balance.
        if (active) setRaw(0n);
      }
    })();
    return () => {
      active = false;
    };
  }, [connection, publicKey, mint, nonce]);

  return { raw, refetch };
}
