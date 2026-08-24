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
import { authApi, getAuthToken, setAuthToken } from "@/lib/api";

interface AuthContextValue {
  /** Whether the backend requires a password to access the admin API. */
  authRequired: boolean | null;
  /** True once we've checked the backend's auth status at least once. */
  ready: boolean;
  /** True if the user is currently authenticated (token present). */
  authenticated: boolean;
  /** Trigger a re-check of the backend's auth status. */
  refreshStatus: () => Promise<void>;
}

const AuthContext = createContext<AuthContextValue | null>(null);

export function AuthProvider({ children }: { children: ReactNode }) {
  const [authRequired, setAuthRequired] = useState<boolean | null>(null);
  const [authenticated, setAuthenticated] = useState<boolean>(
    !!getAuthToken(),
  );
  const [ready, setReady] = useState(false);
  const aliveRef = useRef(true);

  const refreshStatus = useCallback(async () => {
    try {
      const status = await authApi.status();
      if (!aliveRef.current) return;
      setAuthRequired(status.auth_required);
      if (!status.auth_required) {
        // Backend doesn't require auth — clear any stale token so we
        // don't send stale credentials when auth flips back on later.
        setAuthToken(null);
        setAuthenticated(false);
      } else {
        setAuthenticated(!!getAuthToken());
      }
    } catch {
      if (!aliveRef.current) return;
      // If we can't reach the backend at all, don't block the UI — let
      // the existing health provider handle connectivity feedback.
      setAuthRequired(false);
    } finally {
      if (aliveRef.current) setReady(true);
    }
  }, []);

  useEffect(() => {
    aliveRef.current = true;
    // Defer the initial fetch off the effect body to satisfy the
    // `react-hooks/set-state-in-effect` ESLint rule.
    queueMicrotask(() => {
      void refreshStatus();
    });
    return () => {
      aliveRef.current = false;
    };
  }, [refreshStatus]);

  // Re-sync the authenticated flag whenever the token changes
  // (login, logout, or 401 cleanup) or auth-required flips on.
  useEffect(() => {
    function syncAuth() {
      setAuthenticated(!!getAuthToken());
    }
    window.addEventListener("auth:changed", syncAuth);
    window.addEventListener("auth:unauthorized", syncAuth);
    window.addEventListener("storage", syncAuth);
    return () => {
      window.removeEventListener("auth:changed", syncAuth);
      window.removeEventListener("auth:unauthorized", syncAuth);
      window.removeEventListener("storage", syncAuth);
    };
  }, []);

  return (
    <AuthContext.Provider
      value={{ authRequired, ready, authenticated, refreshStatus }}
    >
      {children}
    </AuthContext.Provider>
  );
}

export function useAuth(): AuthContextValue {
  const ctx = useContext(AuthContext);
  if (!ctx) {
    throw new Error("useAuth must be used inside <AuthProvider>");
  }
  return ctx;
}
