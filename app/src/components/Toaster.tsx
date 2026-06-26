import { CheckCircle2, Loader2, X, XCircle } from "lucide-react";
import { useToast } from "@/lib/toast";
import { explorerTx } from "@/lib/config";
import { cn } from "@/lib/utils";

export function Toaster() {
  const { toasts, dismiss } = useToast();
  if (toasts.length === 0) return null;

  return (
    <div className="fixed bottom-5 right-5 z-50 flex w-[min(92vw,360px)] flex-col gap-2">
      {toasts.map((t) => (
        <div
          key={t.id}
          role="status"
          className={cn(
            "panel flex items-start gap-3 px-4 py-3 text-sm animate-rise",
            t.kind === "error" && "border-star/40",
          )}
        >
          <span className="mt-0.5 shrink-0">
            {t.kind === "pending" && <Loader2 className="h-4 w-4 animate-spin text-meridian" />}
            {t.kind === "success" && <CheckCircle2 className="h-4 w-4 text-meridian" />}
            {t.kind === "error" && <XCircle className="h-4 w-4 text-star" />}
          </span>
          <div className="min-w-0 flex-1">
            <p className="text-starlight">{t.message}</p>
            {t.signature && (
              <a
                href={explorerTx(t.signature)}
                target="_blank"
                rel="noreferrer"
                className="mt-0.5 inline-block font-mono text-xs text-meridian hover:underline"
              >
                View on explorer ↗
              </a>
            )}
          </div>
          <button onClick={() => dismiss(t.id)} className="shrink-0 text-dusk transition-colors hover:text-starlight" aria-label="Dismiss">
            <X className="h-4 w-4" />
          </button>
        </div>
      ))}
    </div>
  );
}
