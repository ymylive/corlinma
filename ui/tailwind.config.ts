import type { Config } from "tailwindcss";

// Linear-style redesign. Neutral base + indigo accent. Geist sans/mono fonts
// are injected via `app/layout.tsx` as CSS variables consumed here.
const config: Config = {
  darkMode: ["class"],
  content: [
    "./app/**/*.{ts,tsx}",
    "./components/**/*.{ts,tsx}",
    "./lib/**/*.{ts,tsx}",
  ],
  theme: {
    container: {
      center: true,
      padding: "2rem",
      screens: { "2xl": "1400px" },
    },
    extend: {
      colors: {
        border: "hsl(var(--border))",
        input: "hsl(var(--input))",
        ring: "hsl(var(--ring))",
        background: "hsl(var(--background))",
        foreground: "hsl(var(--foreground))",
        primary: {
          DEFAULT: "hsl(var(--primary))",
          foreground: "hsl(var(--primary-foreground))",
        },
        secondary: {
          DEFAULT: "hsl(var(--secondary))",
          foreground: "hsl(var(--secondary-foreground))",
        },
        destructive: {
          DEFAULT: "hsl(var(--destructive))",
          foreground: "hsl(var(--destructive-foreground))",
        },
        muted: {
          DEFAULT: "hsl(var(--muted))",
          foreground: "hsl(var(--muted-foreground))",
        },
        accent: {
          DEFAULT: "hsl(var(--accent))",
          foreground: "hsl(var(--accent-foreground))",
        },
        "accent-2": "hsl(var(--accent-2))",
        "accent-3": "hsl(var(--accent-3))",
        popover: {
          DEFAULT: "hsl(var(--popover))",
          foreground: "hsl(var(--popover-foreground))",
        },
        card: {
          DEFAULT: "hsl(var(--card))",
          foreground: "hsl(var(--card-foreground))",
        },
        panel: "hsl(var(--panel))",
        surface: "hsl(var(--surface))",
        ok: "hsl(var(--ok))",
        warn: "hsl(var(--warn))",
        err: "hsl(var(--err))",
        state: {
          hover: "hsl(var(--state-hover))",
          focus: "hsl(var(--state-focus))",
          press: "hsl(var(--state-press))",
          loading: "hsl(var(--state-loading))",
          skeleton: "hsl(var(--state-skeleton))",
          empty: "hsl(var(--state-empty))",
          error: "hsl(var(--state-error))",
        },
      },
      backgroundColor: {
        "state-hover": "hsl(var(--state-hover))",
        "state-focus": "hsl(var(--state-focus))",
        "state-press": "hsl(var(--state-press))",
        "state-loading": "hsl(var(--state-loading))",
        "state-skeleton": "hsl(var(--state-skeleton))",
        "state-empty": "hsl(var(--state-empty))",
        "state-error": "hsl(var(--state-error))",
      },
      boxShadow: {
        1: "var(--shadow-1)",
        2: "var(--shadow-2)",
        3: "var(--shadow-3)",
        "glow-primary": "var(--glow-primary)",
      },
      transitionTimingFunction: {
        spring: "cubic-bezier(0.34, 1.56, 0.64, 1)",
      },
      borderRadius: {
        lg: "var(--radius)",
        md: "calc(var(--radius) - 2px)",
        sm: "calc(var(--radius) - 4px)",
      },
      fontFamily: {
        sans: ["var(--font-geist-sans)", "ui-sans-serif", "system-ui", "sans-serif"],
        mono: [
          "var(--font-geist-mono)",
          "ui-monospace",
          "SFMono-Regular",
          "monospace",
        ],
      },
      keyframes: {
        "accordion-down": {
          from: { height: "0" },
          to: { height: "var(--radix-accordion-content-height)" },
        },
        "accordion-up": {
          from: { height: "var(--radix-accordion-content-height)" },
          to: { height: "0" },
        },
        "fade-in": {
          from: { opacity: "0", transform: "translateY(4px)" },
          to: { opacity: "1", transform: "translateY(0)" },
        },
        "pulse-glow": {
          "0%, 100%": { boxShadow: "0 0 0 rgb(var(--accent) / 0)" },
          "50%": { boxShadow: "var(--glow-primary)" },
        },
        "count-up": {
          "0%": { opacity: "0", transform: "translateY(6px)" },
          "60%": { opacity: "1", transform: "translateY(-2px)" },
          "100%": { opacity: "1", transform: "translateY(0)" },
        },
      },
      animation: {
        "accordion-down": "accordion-down 0.2s ease-out",
        "accordion-up": "accordion-up 0.2s ease-out",
        "fade-in": "fade-in 200ms ease-out",
        "pulse-glow": "pulse-glow 2s ease-in-out infinite",
        "count-up": "count-up 400ms cubic-bezier(0.34, 1.56, 0.64, 1)",
      },
    },
  },
  plugins: [require("tailwindcss-animate")],
};

export default config;
