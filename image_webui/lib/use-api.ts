"use client";

import { useCallback, useEffect, useRef, useState } from "react";

interface AsyncState<T> {
  data: T | null;
  loading: boolean;
  error: string | null;
}

interface UseAsyncOptions {
  pollIntervalMs?: number;
}

/**
 * Lightweight data-fetching hook. Calls `fetcher` on mount and optionally on
 * a fixed interval. Returns data, loading flag and a manual refresh callback.
 */
export function useAsync<T>(
  fetcher: () => Promise<T>,
  deps: unknown[] = [],
  options: UseAsyncOptions = {},
) {
  const [state, setState] = useState<AsyncState<T>>({
    data: null,
    loading: true,
    error: null,
  });
  const fetcherRef = useRef(fetcher);
  const intervalRef = useRef(options.pollIntervalMs);

  useEffect(() => {
    fetcherRef.current = fetcher;
    intervalRef.current = options.pollIntervalMs;
  });

  const refresh = useCallback(async () => {
    setState((prev) => ({ ...prev, loading: true, error: null }));
    try {
      const data = await fetcherRef.current();
      setState({ data, loading: false, error: null });
    } catch (err) {
      setState({
        data: null,
        loading: false,
        error: err instanceof Error ? err.message : "加载失败",
      });
    }
  }, []);

  useEffect(() => {
    let cancelled = false;
    let timer: ReturnType<typeof setInterval> | null = null;

    const run = async () => {
      setState((prev) => ({ ...prev, loading: true, error: null }));
      try {
        const data = await fetcherRef.current();
        if (cancelled) return;
        setState({ data, loading: false, error: null });
      } catch (err) {
        if (cancelled) return;
        setState({
          data: null,
          loading: false,
          error: err instanceof Error ? err.message : "加载失败",
        });
      }
    };

    // Standard "fetch on mount" pattern.
    run();
    if (intervalRef.current) {
      timer = setInterval(run, intervalRef.current);
    }
    return () => {
      cancelled = true;
      if (timer) clearInterval(timer);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps);

  return { ...state, refresh };
}