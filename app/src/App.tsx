import { useState } from "react";
import { Nav, type Screen } from "./components/Nav";
import { Landing } from "./screens/Landing";
import { Swap } from "./screens/Swap";
import { Pools } from "./screens/Pools";
import { Positions } from "./screens/Positions";

export default function App() {
  const [screen, setScreen] = useState<Screen>("home");

  if (screen === "home") {
    return (
      <div className="min-h-full">
        <Landing onNavigate={setScreen} />
      </div>
    );
  }

  return (
    <div className="min-h-full">
      <Nav active={screen} onNavigate={setScreen} />
      <main>
        {screen === "swap" && <Swap />}
        {screen === "pools" && <Pools onNavigate={setScreen} />}
        {screen === "positions" && <Positions />}
      </main>
      <footer className="border-t border-line/40 py-6 text-center text-xs text-dusk">
        Zenith · concentrated-liquidity AMM · Solana devnet
      </footer>
    </div>
  );
}
