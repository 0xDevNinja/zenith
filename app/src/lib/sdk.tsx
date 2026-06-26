import { createContext, useContext, useMemo, type ReactNode } from "react";
import { useConnection } from "@solana/wallet-adapter-react";
import { ZenithConnection } from "@zenith/sdk";
import { COMMITMENT, PROGRAM_ID } from "./config";

interface ZenithCtx {
  zenith: ZenithConnection;
  programId: typeof PROGRAM_ID;
}

const Ctx = createContext<ZenithCtx | null>(null);

// Wraps the wallet-adapter Connection in the SDK client and exposes it app-wide.
// Rebuilds only when the underlying connection changes.
export function ZenithProvider({ children }: { children: ReactNode }) {
  const { connection } = useConnection();

  const value = useMemo<ZenithCtx>(
    () => ({
      zenith: new ZenithConnection(connection, { commitment: COMMITMENT }),
      programId: PROGRAM_ID,
    }),
    [connection],
  );

  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

export function useZenith(): ZenithCtx {
  const ctx = useContext(Ctx);
  if (!ctx) throw new Error("useZenith must be used within <ZenithProvider>");
  return ctx;
}
