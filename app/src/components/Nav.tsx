import { Wordmark } from "./Logo";
import { ThemeToggle } from "./ThemeToggle";
import { WalletButton } from "./WalletButton";
import { cn } from "@/lib/utils";

export type Screen = "home" | "swap" | "pools" | "positions";

const TABS: { id: Screen; label: string }[] = [
  { id: "swap", label: "Swap" },
  { id: "pools", label: "Pools" },
  { id: "positions", label: "Positions" },
];

export function Nav({
  active,
  onNavigate,
}: {
  active: Screen;
  onNavigate: (s: Screen) => void;
}) {
  return (
    <header className="sticky top-0 z-20 border-b border-line/50 bg-night/70 backdrop-blur-md">
      <div className="mx-auto flex h-16 max-w-6xl items-center justify-between px-5">
        <button onClick={() => onNavigate("home")} className="group" aria-label="Zenith home">
          <Wordmark />
        </button>

        <nav className="hidden items-center gap-1 rounded-full border border-line/60 bg-panel/50 p-1 sm:flex">
          {TABS.map((t) => (
            <button
              key={t.id}
              onClick={() => onNavigate(t.id)}
              className={cn(
                "rounded-full px-4 py-1.5 text-sm font-medium transition-colors",
                active === t.id
                  ? "bg-panel-2 text-starlight shadow-sm"
                  : "text-dusk hover:text-starlight",
              )}
            >
              {t.label}
            </button>
          ))}
        </nav>

        <div className="flex items-center gap-2">
          <ThemeToggle />
          <WalletButton />
        </div>
      </div>
    </header>
  );
}
