"use client";

import {
  Area,
  AreaChart,
  CartesianGrid,
  Cell,
  Legend,
  Line,
  LineChart,
  Pie,
  PieChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import { useTheme } from "./theme-provider";
import type { SeriesPoint } from "@/lib/api";

const PIE_COLORS = [
  "#3b82f6",
  "#22c55e",
  "#a855f7",
  "#f97316",
  "#ec4899",
  "#06b6d4",
  "#facc15",
];

interface ThemeColors {
  text: string;
  textMuted: string;
  border: string;
  surface: string;
  success: string;
  danger: string;
}

function readColors(): ThemeColors {
  if (typeof window === "undefined") {
    return {
      text: "#fff",
      textMuted: "rgba(255,255,255,0.5)",
      border: "rgba(255,255,255,0.1)",
      surface: "rgba(0,0,0,0.2)",
      success: "#22c55e",
      danger: "#f43f5e",
    };
  }
  const cs = getComputedStyle(document.documentElement);
  return {
    text: cs.getPropertyValue("--text-strong").trim() || "#fff",
    textMuted: cs.getPropertyValue("--text-muted").trim() || "rgba(255,255,255,0.5)",
    border: cs.getPropertyValue("--border").trim() || "rgba(255,255,255,0.1)",
    surface: cs.getPropertyValue("--surface-2").trim() || "rgba(0,0,0,0.2)",
    success: "#22c55e",
    danger: "#f43f5e",
  };
}

function useColors(): ThemeColors {
  const { theme } = useTheme();
  // Re-derive on theme change. useTheme doesn't expose the actual value,
  // but the CSS vars updated by theme change trigger a re-render here.
  void theme;
  return readColors();
}

function fmtTick(ts: number, hours: number): string {
  const d = new Date(ts < 1e12 ? ts * 1000 : ts);
  const pad = (n: number) => n.toString().padStart(2, "0");
  if (hours <= 24) {
    return `${pad(d.getHours())}:${pad(d.getMinutes())}`;
  }
  return `${pad(d.getMonth() + 1)}/${pad(d.getDate())}`;
}

export function RequestAreaChart({
  points,
  hours,
}: {
  points: SeriesPoint[];
  hours: number;
}) {
  const c = useColors();
  const data = (points ?? []).map((p) => ({
    ts: p.ts,
    total: p.total,
    success: p.success,
    error: p.error,
  }));
  return (
    <ResponsiveContainer width="100%" height={260}>
      <AreaChart data={data} margin={{ top: 8, right: 12, left: -8, bottom: 0 }}>
        <defs>
          <linearGradient id="grad-total" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stopColor="#3b82f6" stopOpacity={0.5} />
            <stop offset="100%" stopColor="#3b82f6" stopOpacity={0.05} />
          </linearGradient>
          <linearGradient id="grad-success" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stopColor="#22c55e" stopOpacity={0.4} />
            <stop offset="100%" stopColor="#22c55e" stopOpacity={0.05} />
          </linearGradient>
          <linearGradient id="grad-error" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stopColor="#f43f5e" stopOpacity={0.4} />
            <stop offset="100%" stopColor="#f43f5e" stopOpacity={0.05} />
          </linearGradient>
        </defs>
        <CartesianGrid stroke={c.border} strokeDasharray="3 3" />
        <XAxis
          dataKey="ts"
          tick={{ fill: c.textMuted, fontSize: 11 }}
          tickFormatter={(v) => fmtTick(v as number, hours)}
          minTickGap={32}
        />
        <YAxis tick={{ fill: c.textMuted, fontSize: 11 }} allowDecimals={false} width={36} />
        <Tooltip
          contentStyle={{
            background: c.surface,
            border: `1px solid ${c.border}`,
            borderRadius: 8,
            color: c.text,
          }}
          labelFormatter={(v) => {
            const d = new Date(((v as number) < 1e12 ? (v as number) * 1000 : v) as number);
            return d.toLocaleString("zh-CN");
          }}
        />
        <Legend wrapperStyle={{ fontSize: 11, color: c.textMuted }} />
        <Area
          type="monotone"
          dataKey="total"
          name="请求"
          stroke="#3b82f6"
          fill="url(#grad-total)"
          strokeWidth={1.5}
        />
        <Area
          type="monotone"
          dataKey="success"
          name="成功"
          stroke="#22c55e"
          fill="url(#grad-success)"
          strokeWidth={1.5}
        />
        <Area
          type="monotone"
          dataKey="error"
          name="失败"
          stroke="#f43f5e"
          fill="url(#grad-error)"
          strokeWidth={1.5}
        />
      </AreaChart>
    </ResponsiveContainer>
  );
}

export function LatencyLineChart({
  points,
  hours,
}: {
  points: SeriesPoint[];
  hours: number;
}) {
  const c = useColors();
  const data = (points ?? []).map((p) => ({
    ts: p.ts,
    avg_latency_ms: Math.round(p.avg_latency_ms || 0),
  }));
  return (
    <ResponsiveContainer width="100%" height={200}>
      <LineChart data={data} margin={{ top: 8, right: 12, left: -8, bottom: 0 }}>
        <CartesianGrid stroke={c.border} strokeDasharray="3 3" />
        <XAxis
          dataKey="ts"
          tick={{ fill: c.textMuted, fontSize: 11 }}
          tickFormatter={(v) => fmtTick(v as number, hours)}
          minTickGap={32}
        />
        <YAxis
          tick={{ fill: c.textMuted, fontSize: 11 }}
          width={48}
          tickFormatter={(v) => `${v}ms`}
        />
        <Tooltip
          contentStyle={{
            background: c.surface,
            border: `1px solid ${c.border}`,
            borderRadius: 8,
            color: c.text,
          }}
          labelFormatter={(v) => {
            const d = new Date(((v as number) < 1e12 ? (v as number) * 1000 : v) as number);
            return d.toLocaleString("zh-CN");
          }}
          formatter={(v) => [`${v} ms`, "平均延迟"]}
        />
        <Line
          type="monotone"
          dataKey="avg_latency_ms"
          stroke="#a855f7"
          strokeWidth={2}
          dot={false}
        />
      </LineChart>
    </ResponsiveContainer>
  );
}

interface PieDatum {
  name: string;
  value: number;
}

export function ModelUsagePieChart({ data, top = 5 }: { data: PieDatum[]; top?: number }) {
  const c = useColors();
  const topData = (data ?? []).slice(0, top);
  if (!topData.length) {
    return (
      <div className="flex h-[200px] items-center justify-center text-xs text-[var(--text-faint)]">
        暂无数据
      </div>
    );
  }
  return (
    <ResponsiveContainer width="100%" height={200}>
      <PieChart>
        <Pie
          data={topData}
          dataKey="value"
          nameKey="name"
          cx="50%"
          cy="50%"
          outerRadius={70}
          innerRadius={32}
          paddingAngle={2}
          stroke="none"
        >
          {topData.map((_, i) => (
            <Cell key={i} fill={PIE_COLORS[i % PIE_COLORS.length]} />
          ))}
        </Pie>
        <Tooltip
          contentStyle={{
            background: c.surface,
            border: `1px solid ${c.border}`,
            borderRadius: 8,
            color: c.text,
          }}
        />
        <Legend
          wrapperStyle={{ fontSize: 11, color: c.textMuted }}
          iconSize={8}
        />
      </PieChart>
    </ResponsiveContainer>
  );
}
