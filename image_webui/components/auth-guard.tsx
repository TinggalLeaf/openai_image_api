"use client";

import { usePathname, useRouter } from "next/navigation";
import { useEffect, type ReactNode } from "react";
import { useAuth } from "./auth-provider";

/**
 * Top-level guard: while we don't know the backend's auth posture,
 * show a brief loading state. Once known:
 *   - /login is always reachable
 *   - any other route requires authentication when auth_required
 *   - a global 401 triggers a re-route to /login with `next` set
 */
export function AuthGuard({ children }: { children: ReactNode }) {
  const { authRequired, authenticated, ready } = useAuth();
  const pathname = usePathname();
  const router = useRouter();

  // Listen for 401s from the api client and bounce to /login.
  useEffect(() => {
    function onUnauth() {
      if (pathname !== "/login") {
        const next = pathname && pathname !== "/" ? `?next=${encodeURIComponent(pathname)}` : "";
        router.replace(`/login${next}`);
      }
    }
    window.addEventListener("auth:unauthorized", onUnauth);
    return () => window.removeEventListener("auth:unauthorized", onUnauth);
  }, [pathname, router]);

  if (!ready) {
    return (
      <div className="flex min-h-screen items-center justify-center text-sm text-white/40">
        正在连接后端…
      </div>
    );
  }

  // Login is always allowed.
  if (pathname === "/login") return <>{children}</>;

  if (authRequired && !authenticated) {
    // The guard will redirect via the unauthorized listener or the
    // mount-time check below. Render children only after we know
    // we're authenticated to avoid flashing protected UI.
    if (typeof window !== "undefined") {
      const next = pathname && pathname !== "/" ? `?next=${encodeURIComponent(pathname)}` : "";
      router.replace(`/login${next}`);
    }
    return (
      <div className="flex min-h-screen items-center justify-center text-sm text-white/40">
        跳转到登录…
      </div>
    );
  }

  return <>{children}</>;
}
