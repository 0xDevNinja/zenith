import { useEffect, useRef, useState } from "react";
import { useWallet } from "@solana/wallet-adapter-react";
import { useWalletModal } from "@solana/wallet-adapter-react-ui";
import { Check, Copy, ExternalLink, LogOut, Wallet } from "lucide-react";
import { Button } from "./ui/button";
import { useBalance } from "@/lib/useBalance";
import { explorerAddress } from "@/lib/config";
import { cn } from "@/lib/utils";

function short(addr: string): string {
  return `${addr.slice(0, 4)}…${addr.slice(-4)}`;
}

export function WalletButton() {
  const { publicKey, connected, connecting, disconnect } = useWallet();
  const { setVisible } = useWalletModal();
  const balance = useBalance();
  const [open, setOpen] = useState(false);
  const [copied, setCopied] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const onClick = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", onClick);
    return () => document.removeEventListener("mousedown", onClick);
  }, [open]);

  if (!connected || !publicKey) {
    return (
      <Button
        variant="primary"
        size="sm"
        onClick={() => setVisible(true)}
        disabled={connecting}
        className="font-mono text-xs"
      >
        <Wallet className="h-4 w-4" />
        {connecting ? "Connecting…" : "Connect"}
      </Button>
    );
  }

  const address = publicKey.toBase58();

  const copy = async () => {
    await navigator.clipboard.writeText(address);
    setCopied(true);
    setTimeout(() => setCopied(false), 1200);
  };

  return (
    <div className="relative" ref={ref}>
      <button
        onClick={() => setOpen((o) => !o)}
        className="flex items-center gap-2.5 rounded-2xl border border-line bg-panel/60 py-1.5 pl-2.5 pr-3 transition-colors hover:border-meridian/40"
      >
        <span className="h-2 w-2 rounded-full bg-meridian shadow-[0_0_8px] shadow-meridian" />
        <span className="font-mono text-xs text-starlight tnum">{short(address)}</span>
        {balance !== null && (
          <span className="font-mono text-xs text-dusk tnum">{balance.toFixed(2)} SOL</span>
        )}
      </button>

      {open && (
        <div className="absolute right-0 top-full z-30 mt-2 w-52 overflow-hidden rounded-2xl border border-line bg-panel shadow-instrument">
          <MenuItem onClick={copy} icon={copied ? <Check className="h-4 w-4 text-meridian" /> : <Copy className="h-4 w-4" />}>
            {copied ? "Copied" : "Copy address"}
          </MenuItem>
          <a href={explorerAddress(address)} target="_blank" rel="noreferrer" className="block">
            <MenuItem icon={<ExternalLink className="h-4 w-4" />}>View on explorer</MenuItem>
          </a>
          <MenuItem
            onClick={() => {
              setOpen(false);
              disconnect().catch(() => {});
            }}
            icon={<LogOut className="h-4 w-4" />}
            danger
          >
            Disconnect
          </MenuItem>
        </div>
      )}
    </div>
  );
}

function MenuItem({
  children,
  icon,
  onClick,
  danger,
}: {
  children: React.ReactNode;
  icon: React.ReactNode;
  onClick?: () => void;
  danger?: boolean;
}) {
  return (
    <button
      onClick={onClick}
      className={cn(
        "flex w-full items-center gap-2.5 px-4 py-2.5 text-left text-sm transition-colors hover:bg-panel-2/60",
        danger ? "text-star" : "text-starlight",
      )}
    >
      {icon}
      {children}
    </button>
  );
}
