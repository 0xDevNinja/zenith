export type Theme = "dark" | "light";

// SVG charts paint with literal hex (gradients/strokes can't read Tailwind
// tokens cleanly), so they pull from this per-theme palette. Light values are
// deepened — bright gold and cyan wash out on paper.
export interface ChartPalette {
  barTop: string;
  barBottom: string;
  env: string;
  baseline: string;
  axis: string;
  active: string;
  activeOut: string;
  centerDot: string;
  rangeIn: string;
  rangeOut: string;
  logoTop: string;
  logoBottom: string;
  zenith: string;
}

export const PALETTES: Record<Theme, ChartPalette> = {
  dark: {
    barTop: "#FFC56D",
    barBottom: "#E0A85A",
    env: "#FFC56D",
    baseline: "#265863",
    axis: "#7FA3A8",
    active: "#2DD4C4",
    activeOut: "#FFC56D",
    centerDot: "#0C2630",
    rangeIn: "#2DD4C4",
    rangeOut: "#7FA3A8",
    logoTop: "#FFC56D",
    logoBottom: "#E0A85A",
    zenith: "#2DD4C4",
  },
  light: {
    barTop: "#F0A93C",
    barBottom: "#D8922E",
    env: "#C98A2E",
    baseline: "#CFE0E0",
    axis: "#7C9A9E",
    active: "#11A89C",
    activeOut: "#C98A2E",
    centerDot: "#FFFFFF",
    rangeIn: "#11A89C",
    rangeOut: "#93A8A8",
    logoTop: "#E8A53A",
    logoBottom: "#C98A2E",
    zenith: "#11A89C",
  },
};
