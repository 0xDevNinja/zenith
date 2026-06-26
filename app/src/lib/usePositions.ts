import { useCallback, useEffect, useState } from "react";
import { PublicKey } from "@solana/web3.js";
import { TOKEN_PROGRAM_ID } from "@solana/spl-token";
import { fetchPosition, positionPda, type Position } from "@zenith/sdk";
import { useConnection, useWallet } from "@solana/wallet-adapter-react";
import { useZenith } from "./sdk";
import { MARKET } from "./market";

export interface OwnedPosition {
  address: PublicKey;
  nftMint: PublicKey;
  position: Position;
}

interface PositionsState {
  positions: OwnedPosition[];
  loading: boolean;
  refetch: () => void;
}

// Ownership is hold-the-NFT: enumerate the wallet's single-supply, 0-decimal
// token accounts (candidate position NFTs), derive each position PDA, and keep
// the ones that decode and belong to this market's pool.
export function usePositions(): PositionsState {
  const { connection } = useConnection();
  const { zenith } = useZenith();
  const { publicKey } = useWallet();
  const [positions, setPositions] = useState<OwnedPosition[]>([]);
  const [loading, setLoading] = useState(false);
  const [nonce, setNonce] = useState(0);

  const refetch = useCallback(() => setNonce((n) => n + 1), []);

  useEffect(() => {
    if (!publicKey) {
      setPositions([]);
      return;
    }
    let active = true;
    setLoading(true);
    (async () => {
      try {
        const { value } = await connection.getParsedTokenAccountsByOwner(publicKey, {
          programId: TOKEN_PROGRAM_ID,
        });
        const nftMints = value
          .map((a) => a.account.data.parsed.info)
          .filter((i) => i.tokenAmount.decimals === 0 && i.tokenAmount.amount === "1")
          .map((i) => new PublicKey(i.mint));

        const found = await Promise.all(
          nftMints.map(async (nftMint) => {
            try {
              const address = positionPda(nftMint).address;
              const position = await fetchPosition(zenith, address);
              if (position && position.pool.equals(MARKET.pool)) {
                return { address, nftMint, position } satisfies OwnedPosition;
              }
            } catch {
              // A single candidate failing to load must not drop the rest.
            }
            return null;
          }),
        );
        if (active) setPositions(found.filter((p): p is OwnedPosition => p !== null));
      } catch {
        if (active) setPositions([]);
      } finally {
        if (active) setLoading(false);
      }
    })();
    return () => {
      active = false;
    };
  }, [connection, zenith, publicKey, nonce]);

  return { positions, loading, refetch };
}
