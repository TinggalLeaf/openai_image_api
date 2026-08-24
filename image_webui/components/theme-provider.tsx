"use client";

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useState,
  type ReactNode,
} from "react";

type Theme = "dark" | "light";

interface ThemeContextValue {
  theme: Theme;
  setTheme: (t: Theme) => void;
  toggle: () => void;
}

const ThemeContext = createContext<ThemeContextValue | null>(null);
const STORAGE_KEY = "admin_theme";

function applyTheme(t: Theme) {
  if (typeof document === "undefined") return;
  const html = document.documentElement;
  if (t === "dark") html.classList.add("dark");
  else html.classList.remove("dark");
  html.dataset.theme = t;
}

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [theme, setThemeState] = useState<Theme>("dark");

  // On mount, read the persisted preference (default: dark) and apply.
  useEffect(() => {
    let initial: Theme = "dark";
    try {
      const stored = window.localStorage.getItem(STORAGE_KEY);
      if (stored === "light" || stored === "dark") initial = stored;
    } catch {
      // ignore
    }
    // Defer to satisfy `react-hooks/set-state-in-effect`.
    queueMicrotask(() => {
      setThemeState(initial);
      applyTheme(initial);
    });
  }, []);

  const setTheme = useCallback((t: Theme) => {
    setThemeState(t);
    try {
      window.localStorage.setItem(STORAGE_KEY, t);
    } catch {
      // ignore
    }
    applyTheme(t);
  }, []);

  const toggle = useCallback(() => {
    setTheme(theme === "dark" ? "light" : "dark");
  }, [theme, setTheme]);

  return (
    <ThemeContext.Provider value={{ theme, setTheme, toggle }}>
      {children}
    </ThemeContext.Provider>
  );
}

export function useTheme(): ThemeContextValue {
  const ctx = useContext(ThemeContext);
  if (!ctx) throw new Error("useTheme must be used inside <ThemeProvider>");
  return ctx;
}
