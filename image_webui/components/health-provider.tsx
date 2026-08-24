"use client";

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { healthApi, type HealthResponse } from "@/lib/api";

interface HealthContextValue {
  health: HealthResponse | null;
  loading: boolean;
  error: string | null;
  refresh: () => Promise<void>;
}

const HealthContext = createContext<HealthContextValue | null>(null);

const POLL_INTERVAL = 30_000;

async function fetchHealth(
  aliveRef: React.MutableRefObject<boolean>,
  setHealth: (h: HealthResponse | null) => void,
  setError: (e: string | null) => void,
  setLoading: (l: boolean) => void,
) {
  try {
    const data = await healthApi.get();
    if (!aliveRef.current) return;
    setHealth(data);
    setError(null);
  } catch (err) {
    if (!aliveRef.current) return;
    setError(err instanceof Error ? err.message : "无法连接后端");
    setHealth(null);
  } finally {
    if (aliveRef.current) setLoading(false);
  }
}

export function HealthProvider({ children }: { children: ReactNode }) {
  const [health, setHealth] = useState<HealthResponse | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const aliveRef = useRef(true);

  const refresh = useCallback(
    () => fetchHealth(aliveRef, setHealth, setError, setLoading),
    [],
  );

  useEffect(() => {
    aliveRef.current = true;
    let timer: ReturnType<typeof setInterval> | null = null;
    // Kick off the initial fetch (and the polling interval) inside an effect —
    // this is the canonical data-fetching on mount pattern.
    fetchHealth(aliveRef, setHealth, setError, setLoading);
    timer = setInterval(refresh, POLL_INTERVAL);
    return () => {
      aliveRef.current = false;
      if (timer) clearInterval(timer);
    };
  }, [refresh]);

  return (
    <HealthContext.Provider value={{ health, loading, error, refresh }}>
      {children}
    </HealthContext.Provider>
  );
}

export function useHealth(): HealthContextValue {
  const ctx = useContext(HealthContext);
  if (!ctx) {
    throw new Error("useHealth must be used inside <HealthProvider>");
  }
  return ctx;
}