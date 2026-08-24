"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import { Button, Chip } from "@heroui/react";
import {
  logsApi,
  overviewApi,
  queueApi,
  statsApi,
  storageApi,
  type LogRecord,
  type ModelStats,
  type OverviewResponse,
  type QueueStatus,
  type SeriesResponse,
  type StorageInfo,
} from "@/lib/api";
import { useAsync } from "@/lib/use-api";
import { useHealth } from "@/components/health-provider";
import { Card, EmptyState, ErrorBanner, PageHeader } from "@/components/ui-helpers";
import {
  LatencyLineChart,
  ModelUsagePieChart,
  RequestAreaChart,
} from "@/components/charts";
import {
  formatBytes,
  formatDurationSince,
  formatLatency,
  formatNumber,
  formatTimestamp,
  truncate,
} from "@/lib/utils";

const SERIES_POLL_MS = 30_000;
const STATS_POLL_MS = 60_000;
const WALL_POLL_MS = 15_000;

export default function DashboardPage() {
  const overview = useAsync<OverviewResponse>(
    () => overviewApi.get(),
    [],
    { pollIntervalMs: SERIES_POLL_MS },
  );
  const logs = useAsync<{ items: LogRecord[] }>(
    () => logsApi.list({ page: 1, page_size: 10 }),
    [],
    { pollIntervalMs: SERIES_POLL_MS },
  );
  const queue = useAsync<QueueStatus>(
    () => queueApi.get(),
    [],
    { pollIntervalMs: SERIES_POLL_MS },
  );
  const storage = useAsync<StorageInfo>(
    () => storageApi.get(),
    [],
    { pollIntervalMs: SERIES_POLL_MS },
  );
  const series = useAsync<SeriesResponse>(
    () => statsApi.series({ hours: 24, bucket: "hour" }),
    [],
    { pollIntervalMs: STATS_POLL_MS },
  );
  const modelStats = useAsync<ModelStats[]>(
    () => statsApi.byModel(),
    [],
    { pollIntervalMs: STATS_POLL_MS },
  );

  const { health } = useHealth();
  const comfyuiOnline = !!health?.comfyui_online;

  const successRate =
    overview.data && overview.data.requests_total > 0
      ? Math.round(
          (overview.data.success_count / overview.data.requests_total) * 100,
        )
      : null;

  // Top-5 models for the dashboard pie
  const modelPieData = (modelStats.data ?? [])
    .slice()
    .sort((a, b) => b.total - a.total)
    .map((m) => ({ name: m.model_name || m.model_id, value: m.total }));

  // Wall mode: enter/exit fullscreen, accelerated 15s polling, hide chrome.
  const [wall, setWall] = useState(false);

  useEffect(() => {
    function onFsChange() {
      setWall(!!document.fullscreenElement);
    }
    document.addEventListener("fullscreenchange", onFsChange);
    return () => document.removeEventListener("fullscreenchange", onFsChange);
  }, []);

  const enterWall = async () => {
    try {
      await document.documentElement.requestFullscreen();
    } catch {
      // Some browsers block fullscreen without user gesture; the button
      // still flips the local `wall` flag so the layout hides chrome.
      setWall(true);
    }
  };
  const exitWall = async () => {
    try {
      if (document.fullscreenElement) await document.exitFullscreen();
    } catch {
      // ignore
    }
    setWall(false);
  };

  // Wall-mode poller: re-fetch everything every 15s while active.
  useEffect(() => {
    if (!wall) return;
    const id = setInterval(() => {
      void overview.refresh();
      void logs.refresh();
      void queue.refresh();
      void storage.refresh();
      void series.refresh();
      void modelStats.refresh();
    }, WALL_POLL_MS);
    return () => clearInterval(id);
  }, [wall, overview, logs, queue, storage, series, modelStats]);

  return (
    <div>
      <div className="flex items-center justify-between gap-2 mb-2">
        <PageHeader
          title={wall ? "仪表墙模式" : "仪表盘"}
          description={
            wall
              ? "全屏展示，每 15 秒自动刷新"
              : "网关整体运行状态、最近请求与 ComfyUI 健康度"
          }
        />
        <div className="flex shrink-0 gap-2">
          {wall ? (
            <Button variant="secondary" onPress={exitWall}>
              退出全屏 (ESC)
            </Button>
          ) : (
            <Button variant="tertiary" onPress={enterWall}>
              <span className="inline-flex items-center gap-2">
                <ExpandIcon className="h-4 w-4" />
                全屏
              </span>
            </Button>
          )}
        </div>
      </div>

      {overview.error ? <ErrorBanner message={overview.error} /> : null}

      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-4">
        <StatCard
          label="总请求"
          value={formatNumber(overview.data?.requests_total)}
          loading={overview.loading}
        />
        <StatCard
          label="今日请求"
          value={formatNumber(overview.data?.requests_today)}
          loading={overview.loading}
        />
        <StatCard
          label="成功率"
          value={successRate === null ? "—" : `${successRate}%`}
          subtitle={
            overview.data
              ? `${formatNumber(overview.data.success_count)} / ${formatNumber(overview.data.requests_total)}`
              : undefined
          }
          loading={overview.loading}
        />
        <StatCard
          label="平均延迟"
          value={formatLatency(overview.data?.avg_latency_ms ?? null)}
          loading={overview.loading}
        />
        <StatCard
          label="模型数"
          value={formatNumber(overview.data?.models_count)}
          loading={overview.loading}
        />
        <StatCard
          label="密钥数"
          value={formatNumber(overview.data?.keys_count)}
          loading={overview.loading}
        />
        <StatCard
          label="ComfyUI"
          value={comfyuiOnline ? "在线" : "离线"}
          valueClass={comfyuiOnline ? "text-emerald-400" : "text-rose-400"}
          loading={overview.loading}
        />
        <StatCard
          label="后端版本"
          value={health?.version ?? "—"}
          loading={overview.loading}
        />
      </div>

      {/* Charts row */}
      <div className="mt-6 grid grid-cols-1 gap-4 lg:grid-cols-3">
        <Card className="lg:col-span-2">
          <div className="mb-2 flex items-center justify-between">
            <h3 className="text-sm font-semibold">近 24 小时请求量</h3>
            <span className="text-xs text-[var(--text-faint)]">每小时</span>
          </div>
          {series.error ? (
            <ErrorBanner message={series.error} />
          ) : !series.data && series.loading ? (
            <div className="h-[260px] animate-pulse rounded bg-[var(--surface-2)]" />
          ) : (
            <RequestAreaChart
              points={series.data?.points ?? []}
              hours={24}
            />
          )}
        </Card>
        <Card>
          <div className="mb-2 flex items-center justify-between">
            <h3 className="text-sm font-semibold">模型用量 Top 5</h3>
            <Link href="/stats" className="text-xs text-blue-400 hover:underline">
              统计 →
            </Link>
          </div>
          {modelStats.error ? (
            <ErrorBanner message={modelStats.error} />
          ) : !modelStats.data && modelStats.loading ? (
            <div className="h-[200px] animate-pulse rounded bg-[var(--surface-2)]" />
          ) : (
            <ModelUsagePieChart data={modelPieData} top={5} />
          )}
        </Card>
      </div>

      <div className="mt-4 grid grid-cols-1 gap-4 lg:grid-cols-3">
        <Card>
          <div className="mb-2 flex items-center justify-between">
            <h3 className="text-sm font-semibold">平均延迟趋势</h3>
            <span className="text-xs text-[var(--text-faint)]">近 24 小时</span>
          </div>
          {series.error ? (
            <ErrorBanner message={series.error} />
          ) : !series.data && series.loading ? (
            <div className="h-[200px] animate-pulse rounded bg-[var(--surface-2)]" />
          ) : (
            <LatencyLineChart
              points={series.data?.points ?? []}
              hours={24}
            />
          )}
        </Card>
        <QueueCard queue={queue} />
        <StorageCard storage={storage} />
      </div>

      <div className="mt-4 grid grid-cols-1 gap-4">
        <Card>
          <div className="mb-3 flex items-center justify-between">
            <h3 className="text-sm font-semibold">最近 10 条日志</h3>
            <Link
              href="/logs"
              className="text-xs text-blue-400 hover:underline"
            >
              查看全部 →
            </Link>
          </div>
          {logs.error ? (
            <ErrorBanner message={logs.error} />
          ) : logs.loading && !logs.data ? (
            <div className="space-y-2">
              {Array.from({ length: 5 }).map((_, i) => (
                <div
                  key={i}
                  className="h-8 rounded bg-[var(--surface-2)] animate-pulse"
                />
              ))}
            </div>
          ) : !logs.data?.items.length ? (
            <EmptyState message="暂无日志" />
          ) : (
            <div className="overflow-x-auto">
              <table className="w-full text-sm">
                <thead className="text-left text-xs text-[var(--text-faint)]">
                  <tr>
                    <th className="py-2 pr-2 font-medium">时间</th>
                    <th className="py-2 pr-2 font-medium">模型</th>
                    <th className="py-2 pr-2 font-medium">提示词</th>
                    <th className="py-2 pr-2 font-medium">延迟</th>
                    <th className="py-2 font-medium">状态</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-[var(--border)]">
                  {logs.data.items.map((log) => (
                    <tr key={log.id} className="text-[var(--text)]">
                      <td className="py-2 pr-2 text-xs text-[var(--text-muted)]">
                        {formatTimestamp(log.ts)}
                      </td>
                      <td className="py-2 pr-2 text-xs">
                        {log.model ?? "—"}
                      </td>
                      <td className="py-2 pr-2 text-xs">
                        {truncate(log.prompt, 32) || "—"}
                      </td>
                      <td className="py-2 pr-2 text-xs">
                        {formatLatency(log.latency_ms)}
                      </td>
                      <td className="py-2">
                        <Chip
                          size="sm"
                          variant="soft"
                          color={log.status === "success" ? "success" : "danger"}
                        >
                          {log.status === "success" ? "成功" : "失败"}
                        </Chip>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </Card>
      </div>
    </div>
  );
}

function QueueCard({
  queue,
}: {
  queue: { data: QueueStatus | null; loading: boolean; error: string | null };
}) {
  const data = queue.data;
  const maxConcurrent = data?.max_concurrent ?? 0;
  const running = data?.running ?? 0;
  const queued = data?.queued ?? 0;
  return (
    <Card>
      <div className="mb-3 flex items-center justify-between">
        <h3 className="text-sm font-semibold">队列状态</h3>
        {data ? (
          <span className="text-xs text-[var(--text-faint)]">上限 {maxConcurrent}</span>
        ) : null}
      </div>
      {queue.error ? (
        <ErrorBanner message={queue.error} />
      ) : !data && queue.loading ? (
        <div className="space-y-2">
          <div className="h-6 rounded bg-[var(--surface-2)] animate-pulse" />
          <div className="h-6 rounded bg-[var(--surface-2)] animate-pulse" />
        </div>
      ) : !data ? (
        <div className="text-xs text-[var(--text-faint)]">暂无数据</div>
      ) : (
        <div className="space-y-3 text-sm">
          <div className="grid grid-cols-2 gap-3">
            <div className="rounded-lg border border-[var(--border)] bg-[var(--surface-2)] p-3">
              <div className="text-xs text-[var(--text-faint)]">运行中</div>
              <div className="mt-1 text-lg font-semibold text-[var(--text-strong)]">
                {running}
                <span className="ml-1 text-xs text-[var(--text-faint)]">/ {maxConcurrent}</span>
              </div>
            </div>
            <div className="rounded-lg border border-[var(--border)] bg-[var(--surface-2)] p-3">
              <div className="text-xs text-[var(--text-faint)]">排队中</div>
              <div className="mt-1 text-lg font-semibold text-[var(--text-strong)]">{queued}</div>
            </div>
          </div>
          <QueueSection
            title="运行中"
            items={data.running_items}
            emptyText="空闲"
          />
          <QueueSection
            title="等候中"
            items={data.queued_items}
            emptyText="无排队"
          />
        </div>
      )}
    </Card>
  );
}

function QueueSection({
  title,
  items,
  emptyText,
}: {
  title: string;
  items: { model: string; prompt: string; since_ts: number }[];
  emptyText: string;
}) {
  return (
    <div>
      <div className="mb-1 flex items-center justify-between">
        <span className="text-xs font-medium text-[var(--text-muted)]">{title}</span>
        <span className="text-[11px] text-[var(--text-faint)]">{items.length} 项</span>
      </div>
      {!items.length ? (
        <div className="rounded border border-[var(--border)] bg-[var(--surface-2)] px-3 py-2 text-xs text-[var(--text-faint)]">
          {emptyText}
        </div>
      ) : (
        <ul className="space-y-1">
          {items.slice(0, 5).map((it, i) => (
            <li
              key={`${it.model}-${it.since_ts}-${i}`}
              className="flex items-center justify-between gap-2 rounded border border-[var(--border)] bg-[var(--surface-2)] px-3 py-1.5 text-xs"
            >
              <div className="min-w-0 flex-1">
                <div className="truncate font-mono text-[var(--text-muted)]">{it.model}</div>
                <div className="truncate text-[var(--text-faint)]">{truncate(it.prompt, 36) || "—"}</div>
              </div>
              <span className="shrink-0 text-[11px] text-[var(--text-faint)]">
                {formatDurationSince(it.since_ts)}
              </span>
            </li>
          ))}
          {items.length > 5 ? (
            <li className="text-center text-[11px] text-[var(--text-faint)]">
              还有 {items.length - 5} 项未显示…
            </li>
          ) : null}
        </ul>
      )}
    </div>
  );
}

function StorageCard({
  storage,
}: {
  storage: { data: StorageInfo | null; loading: boolean; error: string | null };
}) {
  const data = storage.data;
  return (
    <Card>
      <div className="mb-3 flex items-center justify-between">
        <h3 className="text-sm font-semibold">图片存储</h3>
        <Link href="/settings" className="text-xs text-blue-400 hover:underline">
          管理 →
        </Link>
      </div>
      {storage.error ? (
        <ErrorBanner message={storage.error} />
      ) : !data && storage.loading ? (
        <div className="space-y-2">
          <div className="h-6 rounded bg-[var(--surface-2)] animate-pulse" />
          <div className="h-6 rounded bg-[var(--surface-2)] animate-pulse" />
        </div>
      ) : !data ? (
        <div className="text-xs text-[var(--text-faint)]">暂无数据</div>
      ) : (
        <div className="space-y-3 text-sm">
          <Row label="文件数">
            <span className="font-mono text-[var(--text-strong)]">
              {formatNumber(data.images_count)}
            </span>
          </Row>
          <Row label="占用空间">
            <span className="font-mono text-[var(--text-strong)]">
              {formatBytes(data.total_bytes)}
            </span>
          </Row>
          <Row label="保留时长">
            <span className="text-[var(--text-strong)]">
              {data.retention_hours === 0 ? "永久" : `${data.retention_hours} 小时`}
            </span>
          </Row>
          <Row label="容量上限">
            <span className="text-[var(--text-strong)]">
              {data.max_total_mb === 0 ? "不限" : `${data.max_total_mb} MB`}
            </span>
          </Row>
          {data.dir ? (
            <div className="truncate rounded bg-[var(--surface-2)] px-2 py-1 text-[11px] text-[var(--text-faint)]" title={data.dir}>
              {data.dir}
            </div>
          ) : null}
        </div>
      )}
    </Card>
  );
}

function StatCard({
  label,
  value,
  subtitle,
  loading,
  valueClass = "",
}: {
  label: string;
  value: React.ReactNode;
  subtitle?: string;
  loading?: boolean;
  valueClass?: string;
}) {
  return (
    <Card>
      <div className="text-xs uppercase tracking-wide text-[var(--text-faint)]">{label}</div>
      <div className={"mt-2 text-2xl font-semibold " + (valueClass || "text-[var(--text-strong)]")}>
        {loading && !value ? (
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

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex items-center justify-between">
      <span className="text-[var(--text-muted)]">{label}</span>
      {children}
    </div>
  );
}

function ExpandIcon(props: React.SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" {...props}>
      <path d="M4 9V5a1 1 0 0 1 1-1h4" />
      <path d="M20 9V5a1 1 0 0 0-1-1h-4" />
      <path d="M4 15v4a1 1 0 0 0 1 1h4" />
      <path d="M20 15v4a1 1 0 0 1-1 1h-4" />
    </svg>
  );
}
