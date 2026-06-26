import { useMemo, type ReactNode } from "react";
import { ConnectionProvider, WalletProvider } from "@solana/wallet-adapter-react";
import { WalletModalProvider } from "@solana/wallet-adapter-react-ui";
import { RPC_ENDPOINT, COMMITMENT } from "@/lib/config";
import { ZenithProvider } from "@/lib/sdk";

import "@solana/wallet-adapter-react-ui/styles.css";

// Wallet + RPC + SDK wiring. Phantom, Backpack, Solflare and friends implement
// the Wallet Standard and register themselves, so the explicit adapter list
// can stay empty — WalletProvider auto-detects them.
export function SolanaProviders({ children }: { children: ReactNode }) {
  const wallets = useMemo(() => [], []);

  return (
    <ConnectionProvider endpoint={RPC_ENDPOINT} config={{ commitment: COMMITMENT }}>
      <WalletProvider wallets={wallets} autoConnect>
        <WalletModalProvider>
          <ZenithProvider>{children}</ZenithProvider>
        </WalletModalProvider>
      </WalletProvider>
    </ConnectionProvider>
  );
}
