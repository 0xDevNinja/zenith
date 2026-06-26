import { ArrowRight, BarChart3, Coins, Gauge, ShieldCheck } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Card, Eyebrow } from "@/components/ui/card";
import { Wordmark } from "@/components/Logo";
import { ThemeToggle } from "@/components/ThemeToggle";
import { WalletButton } from "@/components/WalletButton";
import { DepthChart } from "@/components/DepthChart";
import type { Screen } from "@/components/Nav";
import { cn } from "@/lib/utils";

export function Landing({ onNavigate }: { onNavigate: (s: Screen) => void }) {
  return (
    <div className="relative">
      <LandingHeader onNavigate={onNavigate} />

      {/* Hero */}
      <section className="relative mx-auto max-w-5xl px-5 pb-10 pt-16 text-center sm:pt-24">
        {/* radial glow behind the headline */}
        <div
          aria-hidden
          className="pointer-events-none absolute left-1/2 top-24 -z-0 h-[420px] w-[760px] -translate-x-1/2 rounded-full opacity-60 blur-3xl"
          style={{
            background:
              "radial-gradient(circle at center, rgba(242,200,121,0.18), rgba(95,212,214,0.10) 45%, transparent 70%)",
          }}
        />
        <div className="relative z-10 animate-rise">
          <Eyebrow className="inline-flex items-center gap-2 rounded-full border border-line/70 bg-panel/50 px-3 py-1">
            <span className="h-1.5 w-1.5 rounded-full bg-meridian shadow-[0_0_8px] shadow-meridian" />
            Concentrated-liquidity AMM · Solana devnet
          </Eyebrow>

          <h1 className="mx-auto mt-6 max-w-3xl font-display text-6xl leading-[1.02] tracking-tight text-starlight sm:text-7xl">
            Liquidity at its{" "}
            <span className="relative whitespace-nowrap text-star">
              zenith
              <span className="absolute -bottom-1 left-0 h-px w-full bg-gradient-to-r from-transparent via-star to-transparent" />
            </span>
            .
          </h1>

          <p className="mx-auto mt-6 max-w-xl text-lg leading-relaxed text-dusk">
            An AMM that puts capital where price actually trades. Exact-integer
            math, position NFTs, and dynamic fees — engineered for Solana.
          </p>

          <div className="mt-9 flex items-center justify-center gap-3">
            <Button size="lg" onClick={() => onNavigate("swap")}>
              Launch app
              <ArrowRight className="h-4 w-4" />
            </Button>
            <Button size="lg" variant="outline" onClick={() => onNavigate("pools")}>
              Explore pools
            </Button>
          </div>
        </div>
      </section>

      {/* Hero instrument: live depth chart */}
      <section className="relative mx-auto max-w-5xl px-5">
        <Card className="relative overflow-hidden p-5 sm:p-7">
          <div className="mb-4 flex flex-wrap items-center justify-between gap-3">
            <div className="flex items-center gap-3">
              <span className="grid h-9 w-9 place-items-center rounded-full bg-gradient-to-br from-star/40 to-meridian/20 font-mono text-xs font-semibold">
                SU
              </span>
              <div>
                <div className="font-display text-xl leading-none text-starlight">SOL / USDC</div>
                <div className="font-mono text-xs text-dusk">Live liquidity distribution</div>
              </div>
            </div>
            <div className="flex items-center gap-6">
              <HeroStat label="Active" value="$94.86" tone="meridian" />
              <HeroStat label="TVL" value="$1.24M" />
              <HeroStat label="Fee" value="0.30%" tone="star" />
            </div>
          </div>
          <div className="h-64 w-full sm:h-80">
            <DepthChart activeAt={0.52} width={900} height={340} bins={72} className="h-full w-full" />
          </div>
        </Card>
      </section>

      {/* Stats strip */}
      <section className="mx-auto mt-6 grid max-w-5xl grid-cols-2 gap-3 px-5 sm:grid-cols-4">
        <Metric label="Total liquidity" value="$3.75M" />
        <Metric label="24h volume" value="$2.19M" />
        <Metric label="Active pools" value="4" />
        <Metric label="Math vectors verified" value="21,424" tone="star" />
      </section>

      {/* Features */}
      <section className="mx-auto max-w-5xl px-5 pt-24">
        <div className="mb-10 text-center">
          <Eyebrow>Built on exact mechanics</Eyebrow>
          <h2 className="mt-2 font-display text-4xl text-starlight">Every part earns its place</h2>
        </div>
        <div className="grid gap-4 sm:grid-cols-2">
          <Feature
            icon={<BarChart3 className="h-5 w-5" />}
            title="Concentrated liquidity"
            body="Supply into a price band instead of the whole curve — the same depth for a fraction of the capital."
          />
          <Feature
            icon={<ShieldCheck className="h-5 w-5" />}
            title="Exact-integer math"
            body="Q64.64 fixed point, no floats. Every quote is bit-for-bit reproducible on-chain and off — 21,424 vectors prove it."
          />
          <Feature
            icon={<Coins className="h-5 w-5" />}
            title="Positions as NFTs"
            body="Your liquidity is a token you own. Transfer it, compound fees into it, or close it whenever you like."
          />
          <Feature
            icon={<Gauge className="h-5 w-5" />}
            title="Dynamic fees"
            body="A base schedule plus a volatility surcharge, split across liquidity providers, protocol, and partners."
          />
        </div>
      </section>

      {/* Three steps — a real sequence, so numbering carries meaning. */}
      <section className="mx-auto max-w-5xl px-5 pt-24">
        <div className="mb-10 text-center">
          <Eyebrow>Provide in three steps</Eyebrow>
          <h2 className="mt-2 font-display text-4xl text-starlight">From band to fees</h2>
        </div>
        <div className="grid gap-4 sm:grid-cols-3">
          <Step n="01" title="Pick a price band" body="Choose the range where you expect price to trade. Tighter band, denser liquidity." />
          <Step n="02" title="Provide liquidity" body="Deposit the pair and mint a position NFT scoped to your band." />
          <Step n="03" title="Earn fees" body="Collect trading fees as price moves through your range. Compound or claim anytime." />
        </div>
      </section>

      {/* Final CTA */}
      <section className="mx-auto max-w-5xl px-5 py-24">
        <Card className="relative overflow-hidden px-6 py-14 text-center">
          <div
            aria-hidden
            className="pointer-events-none absolute inset-x-0 top-0 h-40 opacity-50"
            style={{ background: "radial-gradient(600px 200px at 50% 0%, rgba(242,200,121,0.16), transparent)" }}
          />
          <h2 className="relative font-display text-5xl text-starlight">Trade at the peak.</h2>
          <p className="relative mx-auto mt-3 max-w-md text-dusk">
            Swap, provide, and manage positions on Zenith. Running on Solana devnet.
          </p>
          <div className="relative mt-8 flex justify-center">
            <Button size="lg" onClick={() => onNavigate("swap")}>
              Launch app
              <ArrowRight className="h-4 w-4" />
            </Button>
          </div>
          <p className="relative mt-5 font-mono text-xs text-dusk/70">
            Devnet · test funds only · unaudited
          </p>
        </Card>
      </section>

      <footer className="border-t border-line/40 py-8">
        <div className="mx-auto flex max-w-5xl flex-col items-center justify-between gap-3 px-5 text-sm text-dusk sm:flex-row">
          <Wordmark size={24} />
          <span className="font-mono text-xs">Concentrated-liquidity AMM · Solana devnet</span>
        </div>
      </footer>
    </div>
  );
}

function LandingHeader({ onNavigate }: { onNavigate: (s: Screen) => void }) {
  return (
    <header className="sticky top-0 z-20 border-b border-line/40 bg-night/60 backdrop-blur-md">
      <div className="mx-auto flex h-16 max-w-5xl items-center justify-between px-5">
        <Wordmark />
        <nav className="hidden items-center gap-7 text-sm text-dusk sm:flex">
          <button onClick={() => onNavigate("swap")} className="transition-colors hover:text-starlight">Swap</button>
          <button onClick={() => onNavigate("pools")} className="transition-colors hover:text-starlight">Pools</button>
          <button onClick={() => onNavigate("positions")} className="transition-colors hover:text-starlight">Positions</button>
        </nav>
        <div className="flex items-center gap-2">
          <ThemeToggle />
          <WalletButton />
          <Button size="sm" variant="outline" onClick={() => onNavigate("swap")}>
            Launch app
            <ArrowRight className="h-4 w-4" />
          </Button>
        </div>
      </div>
    </header>
  );
}

function HeroStat({ label, value, tone }: { label: string; value: string; tone?: "meridian" | "star" }) {
  return (
    <div className="text-right">
      <div className="text-[10px] uppercase tracking-wider text-dusk">{label}</div>
      <div
        className={cn(
          "mt-0.5 font-mono text-base tnum",
          tone === "meridian" ? "text-meridian" : tone === "star" ? "text-star" : "text-starlight",
        )}
      >
        {value}
      </div>
    </div>
  );
}

function Metric({ label, value, tone }: { label: string; value: string; tone?: "star" }) {
  return (
    <Card className="px-5 py-4">
      <div className="text-xs text-dusk">{label}</div>
      <div className={cn("mt-1 font-mono text-2xl tnum", tone === "star" ? "text-star" : "text-starlight")}>
        {value}
      </div>
    </Card>
  );
}

function Feature({ icon, title, body }: { icon: React.ReactNode; title: string; body: string }) {
  return (
    <Card className="group p-6 transition-transform hover:-translate-y-0.5">
      <div className="mb-4 grid h-11 w-11 place-items-center rounded-xl border border-star/30 bg-star/10 text-star">
        {icon}
      </div>
      <h3 className="font-display text-2xl text-starlight">{title}</h3>
      <p className="mt-2 leading-relaxed text-dusk">{body}</p>
    </Card>
  );
}

function Step({ n, title, body }: { n: string; title: string; body: string }) {
  return (
    <Card className="p-6">
      <div className="font-mono text-sm text-meridian tnum">{n}</div>
      <h3 className="mt-3 font-display text-2xl text-starlight">{title}</h3>
      <p className="mt-2 leading-relaxed text-dusk">{body}</p>
    </Card>
  );
}
