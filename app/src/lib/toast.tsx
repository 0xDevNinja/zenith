import { createContext, useCallback, useContext, useRef, useState, type ReactNode } from "react";
import { decodeTxError } from "./errors";

export type ToastKind = "pending" | "success" | "error";

export interface Toast {
  id: number;
  kind: ToastKind;
  message: string;
  signature?: string;
}

interface ToastCtx {
  toasts: Toast[];
  dismiss: (id: number) => void;
  /// Run a transaction with pending→success/error toasts and decoded errors.
  /// Returns the signature, or null if it failed.
  notifyTx: (run: () => Promise<string>, labels: { pending: string; success: string }) => Promise<string | null>;
}

const Ctx = createContext<ToastCtx | null>(null);

export function ToastProvider({ children }: { children: ReactNode }) {
  const [toasts, setToasts] = useState<Toast[]>([]);
  const seq = useRef(0);

  const dismiss = useCallback((id: number) => {
    setToasts((t) => t.filter((x) => x.id !== id));
  }, []);

  const upsert = useCallback((toast: Toast) => {
    setToasts((t) => [...t.filter((x) => x.id !== toast.id), toast]);
  }, []);

  const notifyTx = useCallback<ToastCtx["notifyTx"]>(
    async (run, labels) => {
      const id = ++seq.current;
      upsert({ id, kind: "pending", message: labels.pending });
      try {
        const signature = await run();
        upsert({ id, kind: "success", message: labels.success, signature });
        setTimeout(() => dismiss(id), 7000);
        return signature;
      } catch (e) {
        upsert({ id, kind: "error", message: decodeTxError(e) });
        setTimeout(() => dismiss(id), 9000);
        return null;
      }
    },
    [upsert, dismiss],
  );

  return <Ctx.Provider value={{ toasts, dismiss, notifyTx }}>{children}</Ctx.Provider>;
}

export function useToast(): ToastCtx {
  const ctx = useContext(Ctx);
  if (!ctx) throw new Error("useToast must be used within <ToastProvider>");
  return ctx;
}
