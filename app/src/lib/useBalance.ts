import { useEffect, useState } from "react";
import { LAMPORTS_PER_SOL } from "@solana/web3.js";
import { useConnection, useWallet } from "@solana/wallet-adapter-react";

// Live SOL balance for the connected wallet. Refetches on account change and on
// a slow interval as a fallback; null while disconnected or loading.
export function useBalance(): number | null {
  const { connection } = useConnection();
  const { publicKey } = useWallet();
  const [sol, setSol] = useState<number | null>(null);

  useEffect(() => {
    if (!publicKey) {
      setSol(null);
      return;
    }
    let active = true;

    const fetch = async () => {
      try {
        const lamports = await connection.getBalance(publicKey);
        if (active) setSol(lamports / LAMPORTS_PER_SOL);
      } catch {
        if (active) setSol(null);
      }
    };

    fetch();
    const sub = connection.onAccountChange(publicKey, (acc) => {
      if (active) setSol(acc.lamports / LAMPORTS_PER_SOL);
    });
    const interval = setInterval(fetch, 30_000);

    return () => {
      active = false;
      clearInterval(interval);
      connection.removeAccountChangeListener(sub).catch(() => {});
    };
  }, [connection, publicKey]);

  return sol;
}
