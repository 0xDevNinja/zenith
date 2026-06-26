import type { Config } from "tailwindcss";

// Colors are driven by CSS variables (see index.css) so the whole palette
// flips between the dark "observatory at night" and light "atlas on paper"
// themes from a single `data-theme` switch. Triplet form keeps Tailwind's
// `/<alpha>` opacity modifiers working.
const v = (name: string) => `rgb(var(--${name}) / <alpha-value>)`;

export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        night: v("night"),
        "night-2": v("night-2"),
        panel: v("panel"),
        "panel-2": v("panel-2"),
        line: v("line"),
        star: v("star"),
        "star-dim": v("star-dim"),
        meridian: v("meridian"),
        starlight: v("starlight"),
        dusk: v("dusk"),
      },
      fontFamily: {
        display: ['"Bricolage Grotesque"', "system-ui", "sans-serif"],
        sans: ['"Hanken Grotesk"', "system-ui", "sans-serif"],
        mono: ['"Geist Mono"', "ui-monospace", "monospace"],
      },
      borderRadius: {
        "4xl": "2rem",
      },
      boxShadow: {
        instrument: "0 1px 0 0 rgba(255,255,255,0.05) inset, 0 24px 60px -30px rgba(0,0,0,0.5)",
        // Friendly aqua CTA glow.
        star: "0 6px 20px -6px rgba(45,212,196,0.5), 0 0 0 1px rgba(45,212,196,0.25)",
      },
      keyframes: {
        "zenith-pulse": {
          "0%, 100%": { opacity: "1", transform: "scale(1)" },
          "50%": { opacity: "0.55", transform: "scale(1.35)" },
        },
        rise: {
          from: { opacity: "0", transform: "translateY(8px)" },
          to: { opacity: "1", transform: "translateY(0)" },
        },
      },
      animation: {
        "zenith-pulse": "zenith-pulse 2.6s ease-in-out infinite",
        rise: "rise 0.5s ease-out both",
      },
    },
  },
  plugins: [],
} satisfies Config;
