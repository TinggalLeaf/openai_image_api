"use client";

import { useState } from "react";
import type { ReactNode } from "react";
import Link from "next/link";
import { usePathname, useRouter } from "next/navigation";
import { Button, Drawer } from "@heroui/react";
import { useHealth } from "./health-provider";
import { useTheme } from "./theme-provider";
import { useAuth } from "./auth-provider";
import { setAuthToken } from "@/lib/api";

const NAV = [
  { href: "/", label: "仪表盘", icon: DashboardIcon },
  { href: "/models", label: "模型管理", icon: ModelsIcon },
  { href: "/keys", label: "API 密钥", icon: KeyIcon },
  { href: "/logs", label: "请求日志", icon: LogsIcon },
  { href: "/stats", label: "统计", icon: StatsIcon },
  { href: "/playground", label: "在线测试", icon: PlayIcon },
  { href: "/settings", label: "设置", icon: SettingsIcon },
];

export function AppShell({ children }: { children: ReactNode }) {
  const pathname = usePathname() ?? "/";
  const router = useRouter();
  const { health, loading } = useHealth();
  const { theme, toggle } = useTheme();
  const { authRequired } = useAuth();
  const [drawerOpen, setDrawerOpen] = useState(false);
  const comfyuiOnline = !!health?.comfyui_online;
  const backendUp = !!health && health.status === "ok";

  const navItems = (
    <nav className="space-y-1">
      {NAV.map((item) => {
        const active =
          item.href === "/"
            ? pathname === "/"
            : pathname === item.href || pathname.startsWith(`${item.href}/`);
        const Icon = item.icon;
        return (
          <Link
            key={item.href}
            href={item.href}
            onClick={() => setDrawerOpen(false)}
            className={
              "flex items-center gap-3 rounded-lg px-3 py-2 text-sm transition-colors " +
              (active
                ? "bg-[var(--surface-3)] text-[var(--text-strong)]"
                : "text-[var(--text-muted)] hover:bg-[var(--surface-2)] hover:text-[var(--text-strong)]")
            }
          >
            <Icon className="h-4 w-4" />
            <span>{item.label}</span>
          </Link>
        );
      })}
    </nav>
  );

  const logout = () => {
    setAuthToken(null);
    router.replace("/login");
  };

  return (
    <div className="flex min-h-screen w-full bg-[var(--background)] text-[var(--foreground)]">
      {/* Desktop sidebar */}
      <aside className="hidden w-60 shrink-0 border-r border-[var(--border)] bg-[var(--surface-1)] md:flex md:flex-col">
        <SidebarBrand />
        <div className="flex-1 px-3 py-4 overflow-y-auto">{navItems}</div>
        <div className="border-t border-[var(--border)] px-5 py-3 text-[11px] text-[var(--text-faint)]">
          v1.0 · ComfyUI Gateway
        </div>
      </aside>

      {/* Mobile drawer */}
      <Drawer isOpen={drawerOpen} onOpenChange={setDrawerOpen}>
        <Drawer.Backdrop>
          <Drawer.Content placement="left" className="!w-64 !max-w-[80vw]">
            <Drawer.Dialog>
              <Drawer.Header>
                <Drawer.Heading>
                  <SidebarBrand />
                </Drawer.Heading>
              </Drawer.Header>
              <Drawer.Body className="space-y-1 px-3 py-4 overflow-y-auto">
                {navItems}
              </Drawer.Body>
            </Drawer.Dialog>
          </Drawer.Content>
        </Drawer.Backdrop>
      </Drawer>

      <div className="flex flex-1 flex-col min-w-0">
        <header className="flex h-14 items-center justify-between border-b border-[var(--border)] bg-[var(--surface-1)] px-3 md:px-6">
          <div className="flex items-center gap-2 min-w-0">
            <button
              type="button"
              onClick={() => setDrawerOpen(true)}
              className="rounded-md p-2 text-[var(--text-muted)] hover:bg-[var(--surface-2)] md:hidden"
              aria-label="打开菜单"
            >
              <MenuIcon className="h-5 w-5" />
            </button>
            <div className="truncate text-sm text-[var(--text-muted)]">
              {NAV.find((n) =>
                n.href === "/"
                  ? pathname === "/"
                  : pathname === n.href || pathname.startsWith(`${n.href}/`),
              )?.label ?? "首页"}
            </div>
          </div>
          <div className="flex items-center gap-3">
            <StatusPill
              label="ComfyUI"
              online={comfyuiOnline}
              loading={loading}
            />
            <StatusPill
              label="后端"
              online={backendUp}
              loading={loading}
            />
            <button
              type="button"
              onClick={toggle}
              aria-label="切换主题"
              title={theme === "dark" ? "切换到亮色" : "切换到暗色"}
              className="flex h-8 items-center gap-1 rounded-lg border border-[var(--border)] bg-[var(--surface-2)] px-2.5 text-xs font-medium text-[var(--text)] hover:bg-[var(--surface-3)]"
            >
              {theme === "dark" ? (
                <SunIcon className="h-4 w-4" />
              ) : (
                <MoonIcon className="h-4 w-4" />
              )}
            </button>
            {authRequired ? (
              <button
                type="button"
                onClick={logout}
                title="退出登录"
                className="flex h-8 items-center rounded-lg border border-[var(--border)] bg-[var(--surface-2)] px-3 text-xs font-medium text-[var(--text)] hover:bg-[var(--surface-3)]"
              >
                退出
              </button>
            ) : null}
          </div>
        </header>
        <main className="flex-1 overflow-x-hidden p-4 md:p-6">{children}</main>
      </div>
    </div>
  );
}

function SidebarBrand() {
  return (
    <div className="flex items-center gap-2 px-5 py-5 border-b border-[var(--border)]">
      <div className="h-8 w-8 rounded-lg bg-blue-600 flex items-center justify-center text-white font-bold">
        IG
      </div>
      <div>
        <div className="text-sm font-semibold">图像生成网关</div>
        <div className="text-[11px] text-[var(--text-faint)]">管理控制台</div>
      </div>
    </div>
  );
}

function StatusPill({
  label,
  online,
  loading,
}: {
  label: string;
  online: boolean;
  loading: boolean;
}) {
  return (
    <div className="hidden items-center gap-2 rounded-full border border-[var(--border)] bg-[var(--surface-2)] px-3 py-1 text-xs sm:flex">
      <span
        className={
          "h-2 w-2 rounded-full " +
          (loading
            ? "bg-[var(--text-faint)] animate-pulse"
            : online
              ? "bg-emerald-400"
              : "bg-rose-500")
        }
      />
      <span className="text-[var(--text-muted)]">{label}</span>
      <span className="font-medium text-[var(--text-strong)]">
        {loading ? "检测中" : online ? "在线" : "离线"}
      </span>
    </div>
  );
}

function MenuIcon(props: React.SVGProps<SVGSVGElement>) {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.8"
      strokeLinecap="round"
      {...props}
    >
      <path d="M4 7h16" />
      <path d="M4 12h16" />
      <path d="M4 17h16" />
    </svg>
  );
}
function SunIcon(props: React.SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" {...props}>
      <circle cx="12" cy="12" r="4" />
      <path d="M12 2v2M12 20v2M4.93 4.93l1.41 1.41M17.66 17.66l1.41 1.41M2 12h2M20 12h2M4.93 19.07l1.41-1.41M17.66 6.34l1.41-1.41" />
    </svg>
  );
}
function MoonIcon(props: React.SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" {...props}>
      <path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z" />
    </svg>
  );
}
function DashboardIcon(props: React.SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" {...props}>
      <rect x="3" y="3" width="7" height="9" rx="1" />
      <rect x="14" y="3" width="7" height="5" rx="1" />
      <rect x="14" y="12" width="7" height="9" rx="1" />
      <rect x="3" y="16" width="7" height="5" rx="1" />
    </svg>
  );
}
function ModelsIcon(props: React.SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" {...props}>
      <path d="M12 3l8 4-8 4-8-4 8-4z" />
      <path d="M4 11l8 4 8-4" />
      <path d="M4 15l8 4 8-4" />
    </svg>
  );
}
function KeyIcon(props: React.SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" {...props}>
      <circle cx="8" cy="15" r="4" />
      <path d="M11 12l9-9" />
      <path d="M16 7l3 3" />
      <path d="M18 5l3 3" />
    </svg>
  );
}
function LogsIcon(props: React.SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" {...props}>
      <path d="M4 5h16" />
      <path d="M4 12h16" />
      <path d="M4 19h10" />
    </svg>
  );
}
function PlayIcon(props: React.SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" {...props}>
      <path d="M6 4l14 8-14 8V4z" />
    </svg>
  );
}
function StatsIcon(props: React.SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" {...props}>
      <path d="M4 20V10" />
      <path d="M10 20V4" />
      <path d="M16 20v-7" />
      <path d="M22 20H2" />
    </svg>
  );
}
function SettingsIcon(props: React.SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" {...props}>
      <circle cx="12" cy="12" r="3" />
      <path d="M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1A2 2 0 1 1 4.3 17l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1A2 2 0 1 1 7 4.3l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1A2 2 0 1 1 19.7 7l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z" />
    </svg>
  );
}
