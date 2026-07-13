import type { Config } from "tailwindcss";

/**
 * Semantic design tokens only — components never hardcode hex values.
 * The actual palette lives in src/styles/tokens.css as CSS custom properties,
 * which keeps a future dark theme a stylesheet swap rather than a refactor.
 */
export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        app: "var(--background-app)",
        sidebar: "var(--background-sidebar)",
        surface: "var(--background-surface)",
        soft: "var(--background-soft)",
        // The active-chip fill. Three call sites already used this name;
        // pixel forensics revealed the token was never defined.
        accent: "var(--background-sage-soft)",
        "blue-soft": "var(--background-blue-soft)",
        "sage-soft": "var(--background-sage-soft)",
        "blush-soft": "var(--background-blush-soft)",
        "muted-blue": "var(--muted-blue)",
        "muted-blue-deep": "var(--muted-blue-deep)",
        sage: "var(--sage)",
        "sage-deep": "var(--sage-deep)",
        "dusty-rose": "var(--dusty-rose)",
        "lavender-muted": "var(--lavender-muted)",
        cream: "var(--cream)",
        ink: "var(--text-primary)",
        "ink-secondary": "var(--text-secondary)",
        "ink-muted": "var(--text-muted)",
        gold: "var(--gold)",
        "gold-deep": "var(--gold-deep)",
        "sidebar-ink": "var(--sidebar-ink)",
        "sidebar-ink-muted": "var(--sidebar-ink-muted)",
        "sidebar-hover": "var(--sidebar-hover)",
        "sidebar-active": "var(--sidebar-active)",
        "border-subtle": "var(--border-subtle)",
        "border-strong": "var(--border-strong)",
        success: "var(--success)",
        warning: "var(--warning)",
        danger: "var(--danger)",
        info: "var(--info)"
      },
      fontFamily: {
        display: "var(--font-display)"
      },
      borderRadius: {
        card: "10px",
        control: "8px"
      },
      boxShadow: {
        card: "0 1px 3px rgba(41, 50, 56, 0.06), 0 1px 2px rgba(41, 50, 56, 0.04)",
        raised: "0 4px 12px rgba(41, 50, 56, 0.08)"
      }
    }
  },
  plugins: []
} satisfies Config;
