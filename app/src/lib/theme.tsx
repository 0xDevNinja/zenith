import { createContext, useContext, useEffect, useState, type ReactNode } from "react";
import { PALETTES, type ChartPalette, type Theme } from "./palette";

interface ThemeCtx {
  theme: Theme;
  palette: ChartPalette;
  toggle: () => void;
}

const Ctx = createContext<ThemeCtx>({
  theme: "dark",
  palette: PALETTES.dark,
  toggle: () => {},
});

const STORAGE_KEY = "zenith-theme";

function initialTheme(): Theme {
  if (typeof localStorage !== "undefined") {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (saved === "light" || saved === "dark") return saved;
  }
  return "dark";
}

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [theme, setTheme] = useState<Theme>(initialTheme);

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    localStorage.setItem(STORAGE_KEY, theme);
  }, [theme]);

  const value: ThemeCtx = {
    theme,
    palette: PALETTES[theme],
    toggle: () => setTheme((t) => (t === "dark" ? "light" : "dark")),
  };

  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

export function useTheme() {
  return useContext(Ctx);
}
