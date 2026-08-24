"use client";

import { Suspense, useEffect, useState } from "react";
import { useRouter, useSearchParams } from "next/navigation";
import { Button } from "@heroui/react";
import { authApi, setAuthToken } from "@/lib/api";
import { useAuth } from "@/components/auth-provider";

function LoginInner() {
  const router = useRouter();
  const params = useSearchParams();
  const next = params.get("next") || "/";
  const { authRequired, authenticated, refreshStatus } = useAuth();
  const [password, setPassword] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // If the backend has no password configured, send the user straight
  // to the dashboard instead of showing a useless form.
  useEffect(() => {
    if (authRequired === false) {
      router.replace(next);
    } else if (authenticated) {
      router.replace(next);
    }
  }, [authRequired, authenticated, next, router]);

  const submit = async (e?: React.FormEvent) => {
    e?.preventDefault();
    if (submitting) return;
    if (!password.trim()) {
      setError("请输入管理密码");
      return;
    }
    setSubmitting(true);
    setError(null);
    try {
      const res = await authApi.login(password);
      setAuthToken(res.token);
      // Pull the latest auth status so AuthGuard knows we're in.
      await refreshStatus();
      router.replace(next);
    } catch (err) {
      setError(err instanceof Error ? err.message : "登录失败");
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div className="flex min-h-screen items-center justify-center px-4">
      <form
        onSubmit={submit}
        className="w-full max-w-sm rounded-2xl border border-white/10 bg-black/30 p-7 shadow-xl backdrop-blur"
      >
        <div className="mb-6 flex items-center gap-3">
          <div className="flex h-10 w-10 items-center justify-center rounded-lg bg-blue-600 text-sm font-bold text-white">
            IG
          </div>
          <div>
            <div className="text-base font-semibold">图像生成网关</div>
            <div className="text-xs text-white/40">管理控制台登录</div>
          </div>
        </div>

        <label className="mb-1.5 block text-xs font-medium text-white/70">
          管理密码
        </label>
        <input
          autoFocus
          type="password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          disabled={submitting}
          placeholder="请输入 ADMIN_PASSWORD"
          className="block w-full rounded-lg border border-white/10 bg-black/30 px-3 py-2 text-sm text-white placeholder:text-white/40 outline-none transition-colors focus:border-blue-500/60 focus:bg-black/40 focus:ring-2 focus:ring-blue-500/20 disabled:opacity-50"
        />

        {error ? (
          <div className="mt-3 rounded-lg border border-rose-500/30 bg-rose-500/10 px-3 py-2 text-xs text-rose-200">
            {error}
          </div>
        ) : null}

        <Button
          type="submit"
          variant="primary"
          fullWidth
          isDisabled={submitting || !password}
          className="mt-4"
          onPress={() => submit()}
        >
          {submitting ? "登录中…" : "登录"}
        </Button>

        <p className="mt-4 text-center text-[11px] text-white/40">
          登录后可访问网关管理接口
        </p>
      </form>
    </div>
  );
}

export default function LoginPage() {
  return (
    <Suspense
      fallback={
        <div className="flex min-h-screen items-center justify-center text-sm text-white/40">
          加载中…
        </div>
      }
    >
      <LoginInner />
    </Suspense>
  );
}
