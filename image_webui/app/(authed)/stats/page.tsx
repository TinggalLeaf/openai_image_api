"use client";

import { useState } from "react";
import {
  statsApi,
  type KeyStats,
  type ModelStats,
  type SeriesResponse,
} from "@/lib/api";
import { useAsync } from "@/lib/use-api";
import { Card, EmptyState, ErrorBanner, PageHeader } from "@/components/ui-helpers";
import {
  LatencyLineChart,
  RequestAreaChart,
} from "@/components/charts";
import {
  formatLatency,
  formatNumber,
  formatTimestamp,
} from "@/lib/utils";

type RangeKey = "1h" | "6h" | "24h" | "7d";

interface RangeSpec {
  hours: number;
  bucket: "minute" | "hour" | "day";
  label: string;
}

const RANGES: Record<RangeKey, RangeSpec> = {
  "1h": { hours: 1, bucket: "minute", label: "近 1 小时" },
  "6h": { hours: 6, bucket: "minute", label: "近 6 小时" },
  "24h": { hours: 24, bucket: "hour", label: "近 24 小时" },
  "7d": { hours: 24 * 7, bucket: "day", label: "近 7 天" },
};

export default function StatsPage() {
  const [range, setRange] = useState<RangeKey>("24h");
  const spec = RANGES[range];

  const series = useAsync<SeriesResponse>(
    () => statsApi.series({ hours: spec.hours, bucket: spec.bucket }),
    [range],
    { pollIntervalMs: 60_000 },
  );
  const keysStats = useAsync<KeyStats[]>(
    () => statsApi.byKey(),
    [],
    { pollIntervalMs: 60_000 },
  );
  const modelsStats = useAsync<ModelStats[]>(
    () => statsApi.byModel(),
    [],
    { pollIntervalMs: 60_000 },
  );

  const points = series.data?.points ?? [];
  const totals = points.reduce(
    (acc, p) => ({
      total: acc.total + p.total,
      success: acc.success + p.success,
      error: acc.error + p.error,
    }),
    { total: 0, success: 0, error: 0 },
  );
  const overallSuccess =
    totals.total > 0 ? Math.round((totals.success / totals.total) * 100) : null;

  return (
    <div>
      <PageHeader
        title="统计"
        description="请求量、延迟趋势，以及按密钥 / 按模型的用量统计"
        actions={
          <div className="inline-flex rounded-lg border border-[var(--border)] bg-[var(--surface-2)] p-0.5">
            {(Object.keys(RANGES) as RangeKey[]).map((k) => (
              <button
                key={k}
                type="button"
                onClick={() => setRange(k)}
                className={
                  "rounded-md px-3 py-1 text-xs font-medium transition-colors " +
                  (range === k
                    ? "bg-[var(--surface-3)] text-[var(--text-strong)]"
                    : "text-[var(--text-muted)] hover:text-[var(--text-strong)]")
                }
              >
                {RANGES[k].label}
              </button>
            ))}
          </div>
        }
      />

      {/* Summary row */}
      <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
        <SummaryCard
          label="请求量"
          value={formatNumber(totals.total)}
          loading={series.loading}
        />
        <SummaryCard
          label="成功率"
          value={overallSuccess === null ? "—" : `${overallSuccess}%`}
          subtitle={`成功 ${formatNumber(totals.success)} / 失败 ${formatNumber(totals.error)}`}
          loading={series.loading}
        />
        <SummaryCard
          label="平均延迟"
          value={
            totals.total > 0
              ? formatLatency(
                  points.reduce((s, p) => s + p.avg_latency_ms * p.total, 0) /
                    Math.max(1, totals.total),
                )
              : "—"
          }
          loading={series.loading}
        />
        <SummaryCard
          label="数据点"
          value={points.length}
          loading={series.loading}
        />
      </div>

      {/* Trend row */}
      <div className="mt-4 grid grid-cols-1 gap-4 lg:grid-cols-3">
        <Card className="lg:col-span-2">
          <div className="mb-2 flex items-center justify-between">
            <h3 className="text-sm font-semibold">请求量趋势</h3>
            <span className="text-xs text-[var(--text-faint)]">{spec.label}</span>
          </div>
          {series.error ? (
            <ErrorBanner message={series.error} />
          ) : !series.data && series.loading ? (
            <div className="h-[260px] animate-pulse rounded bg-[var(--surface-2)]" />
          ) : (
            <RequestAreaChart points={points} hours={spec.hours} />
          )}
        </Card>
        <Card>
          <div className="mb-2 flex items-center justify-between">
            <h3 className="text-sm font-semibold">平均延迟趋势</h3>
            <span className="text-xs text-[var(--text-faint)]">{spec.label}</span>
          </div>
          {series.error ? (
            <ErrorBanner message={series.error} />
          ) : !series.data && series.loading ? (
            <div className="h-[200px] animate-pulse rounded bg-[var(--surface-2)]" />
          ) : (
            <LatencyLineChart points={points} hours={spec.hours} />
          )}
        </Card>
      </div>

      {/* By key */}
      <div className="mt-4 grid grid-cols-1 gap-4 lg:grid-cols-2">
        <StatsTable
          title="按密钥统计"
          rows={keysStats.data ?? []}
          loading={keysStats.loading}
          error={keysStats.error}
          columns={[
            {
              key: "key_name",
              header: "密钥",
              render: (r) => r.key_name,
              cellClassName: "font-medium",
            },
            {
              key: "total",
              header: "请求",
              render: (r) => formatNumber(r.total),
              cellClassName: "text-right tabular-nums",
            },
            {
              key: "success_rate",
              header: "成功率",
              render: (r) =>
                r.total > 0
                  ? `${Math.round((r.success / r.total) * 100)}%`
                  : "—",
              cellClassName: "text-right tabular-nums",
            },
            {
              key: "avg_latency_ms",
              header: "平均延迟",
              render: (r) => formatLatency(r.avg_latency_ms),
              cellClassName: "text-right tabular-nums",
            },
            {
              key: "last_used_at",
              header: "最近使用",
              render: (r) =>
                r.last_used_at ? formatTimestamp(r.last_used_at) : "—",
              cellClassName: "text-right text-xs",
            },
          ]}
        />

        <StatsTable
          title="按模型统计"
          rows={modelsStats.data ?? []}
          loading={modelsStats.loading}
          error={modelsStats.error}
          columns={[
            {
              key: "model_name",
              header: "模型",
              render: (r) => r.model_name,
              cellClassName: "font-medium",
            },
            {
              key: "total",
              header: "请求",
              render: (r) => formatNumber(r.total),
              cellClassName: "text-right tabular-nums",
            },
            {
              key: "success_rate",
              header: "成功率",
              render: (r) =>
                r.total > 0
                  ? `${Math.round((r.success / r.total) * 100)}%`
                  : "—",
              cellClassName: "text-right tabular-nums",
            },
            {
              key: "error",
              header: "失败",
              render: (r) => formatNumber(r.error),
              cellClassName: "text-right tabular-nums",
            },
            {
              key: "avg_latency_ms",
              header: "平均延迟",
              render: (r) => formatLatency(r.avg_latency_ms),
              cellClassName: "text-right tabular-nums",
            },
          ]}
        />
      </div>
    </div>
  );
}

interface ColumnDef<T> {
  key: string;
  header: string;
  render: (row: T) => React.ReactNode;
  cellClassName?: string;
}

function StatsTable<T extends { total: number }>({
  title,
  rows,
  loading,
  error,
  columns,
}: {
  title: string;
  rows: T[];
  loading: boolean;
  error: string | null;
  columns: Array<ColumnDef<T>>;
}) {
  const sorted = [...rows].sort((a, b) => b.total - a.total);
  return (
    <Card className="overflow-x-auto p-0">
      <div className="flex items-center justify-between p-4 pb-2">
        <h3 className="text-sm font-semibold">{title}</h3>
        <span className="text-xs text-[var(--text-faint)]">共 {sorted.length} 条</span>
      </div>
      {error ? (
        <ErrorBanner message={error} />
      ) : loading && sorted.length === 0 ? (
        <div className="space-y-2 p-4">
          {Array.from({ length: 4 }).map((_, i) => (
            <div key={i} className="h-6 rounded bg-[var(--surface-2)] animate-pulse" />
          ))}
        </div>
      ) : sorted.length === 0 ? (
        <EmptyState message="暂无数据" />
      ) : (
        <table className="w-full text-sm">
          <thead>
            <tr className="text-left text-xs text-[var(--text-faint)] border-b border-[var(--border)]">
              {columns.map((c) => (
                <th
                  key={c.key}
                  className={
                    "px-4 py-2 font-medium " +
                    (c.cellClassName?.includes("text-right") ? "text-right" : "")
                  }
                >
                  {c.header}
                </th>
              ))}
            </tr>
          </thead>
          <tbody className="divide-y divide-[var(--border)]">
            {sorted.map((row, i) => (
              <tr key={i} className="text-[var(--text)]">
                {columns.map((c) => (
                  <td
                    key={c.key}
                    className={"px-4 py-2 " + (c.cellClassName ?? "")}
                  >
                    {c.render(row)}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </Card>
  );
}

function SummaryCard({
  label,
  value,
  subtitle,
  loading,
}: {
  label: string;
  value: React.ReactNode;
  subtitle?: string;
  loading?: boolean;
}) {
  return (
    <Card>
      <div className="text-xs uppercase tracking-wide text-[var(--text-faint)]">{label}</div>
      <div className="mt-2 text-2xl font-semibold text-[var(--text-strong)]">
        {loading ? (
          <span className="inline-block h-7 w-20 rounded bg-[var(--surface-2)] animate-pulse" />
        ) : (
          value
        )}
      </div>
      {subtitle ? (
        <div className="mt-1 text-xs text-[var(--text-faint)]">{subtitle}</div>
      ) : null}
    </Card>
  );
}
