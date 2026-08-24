"use client";

import { useEffect, useMemo, useState } from "react";
import { Button, Chip, Drawer } from "@heroui/react";
import {
  logsApi,
  modelsApi,
  type LogRecord,
  type ModelRecord,
  type LogListResponse,
  backendImageUrl,
} from "@/lib/api";
import { useAsync } from "@/lib/use-api";
import {
  Card,
  EmptyState,
  ErrorBanner,
  PageHeader,
} from "@/components/ui-helpers";
import { ConfirmModal } from "@/components/confirm-modal";
import {
  formatLatency,
  formatTimestamp,
  truncate,
  tryPrettyJson,
} from "@/lib/utils";
import { NativeSelect } from "@/components/ui-form";

const PAGE_SIZE = 20;

export default function LogsPage() {
  const [page, setPage] = useState(1);
  const [status, setStatus] = useState<"" | "success" | "error">("");
  const [modelId, setModelId] = useState("");
  const [refreshTick, setRefreshTick] = useState(0);
  const [active, setActive] = useState<LogRecord | null>(null);
  const [detail, setDetail] = useState<LogRecord | null>(null);
  const [detailError, setDetailError] = useState<string | null>(null);
  const [clearOpen, setClearOpen] = useState(false);

  const models = useAsync<ModelRecord[]>(() => modelsApi.list(), []);

  const filterKey = useMemo(
    () => `${page}|${status}|${modelId}|${refreshTick}`,
    [page, status, modelId, refreshTick],
  );

  const logs = useAsync<LogListResponse>(
    () =>
      logsApi.list({
        page,
        page_size: PAGE_SIZE,
        status: status || undefined,
        model: modelId || undefined,
      }),
    [filterKey],
    { pollIntervalMs: 30_000 },
  );

  useEffect(() => {
    if (!active) return;
    let cancelled = false;
    (async () => {
      try {
        const data = await logsApi.get(active.id);
        if (!cancelled) setDetail(data);
      } catch (err) {
        if (!cancelled)
          setDetailError(err instanceof Error ? err.message : "加载失败");
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [active]);

  const totalPages = logs.data
    ? Math.max(1, Math.ceil(logs.data.total / PAGE_SIZE))
    : 1;

  const modelOptions = [
    { value: "all", label: "全部" },
    ...(models.data?.map((m) => ({ value: m.id, label: m.name })) ?? []),
  ];

  return (
    <div>
      <PageHeader
        title="请求日志"
        description="每次调用都会记录在此，包含请求体、响应状态与耗时"
        actions={
          <>
            <Button
              variant="tertiary"
              onPress={() => setRefreshTick((n) => n + 1)}
            >
              刷新
            </Button>
            <Button
              variant="danger"
              onPress={() => setClearOpen(true)}
              isDisabled={!logs.data || logs.data.total === 0}
            >
              清空日志
            </Button>
          </>
        }
      />

      <Card className="mb-4">
        <div className="flex flex-wrap items-end gap-3">
          <NativeSelect
            label="状态"
            value={status || "all"}
            onValueChange={(v) => {
              setPage(1);
              setStatus(v === "all" ? "" : (v as "success" | "error"));
            }}
            options={[
              { value: "all", label: "全部" },
              { value: "success", label: "成功" },
              { value: "error", label: "失败" },
            ]}
            className="w-36"
          />
          <NativeSelect
            label="模型"
            value={modelId || "all"}
            onValueChange={(v) => {
              setPage(1);
              setModelId(v === "all" ? "" : v);
            }}
            options={modelOptions}
            className="w-56"
            disabled={!models.data?.length}
          />
          <div className="ml-auto pb-1 text-xs text-white/40">
            共 {logs.data?.total ?? "—"} 条 · 第 {page} / {totalPages} 页
          </div>
        </div>
      </Card>

      {logs.error ? <ErrorBanner message={logs.error} /> : null}

      {logs.loading && !logs.data ? (
        <div className="space-y-2">
          {Array.from({ length: 5 }).map((_, i) => (
            <div key={i} className="h-12 rounded bg-white/[0.03] animate-pulse" />
          ))}
        </div>
      ) : !logs.data?.items.length ? (
        <Card>
          <EmptyState message="暂无日志" />
        </Card>
      ) : (
        <Card className="overflow-x-auto p-0">
          <table className="w-full text-sm">
            <thead>
              <tr className="text-left text-xs text-white/40 border-b border-white/5">
                <th className="px-4 py-3 font-medium">时间</th>
                <th className="px-4 py-3 font-medium">来源</th>
                <th className="px-4 py-3 font-medium">模型</th>
                <th className="px-4 py-3 font-medium">提示词</th>
                <th className="px-4 py-3 font-medium">耗时</th>
                <th className="px-4 py-3 font-medium">状态</th>
                <th className="px-4 py-3 font-medium text-right">操作</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-white/5">
              {logs.data.items.map((log) => (
                <tr
                  key={log.id}
                  className="cursor-pointer text-white/80 hover:bg-white/[0.02]"
                  onClick={() => setActive(log)}
                >
                  <td className="px-4 py-3 text-xs text-white/50">
                    {formatTimestamp(log.ts)}
                  </td>
                  <td className="px-4 py-3 text-xs">
                    <Chip size="sm" variant="soft">
                      {log.provider}
                    </Chip>
                  </td>
                  <td className="px-4 py-3 text-xs">{log.model ?? "—"}</td>
                  <td className="px-4 py-3 text-xs max-w-xs">
                    {truncate(log.prompt, 50) || "—"}
                  </td>
                  <td className="px-4 py-3 text-xs">
                    {formatLatency(log.latency_ms)}
                  </td>
                  <td className="px-4 py-3">
                    <Chip
                      size="sm"
                      variant="soft"
                      color={log.status === "success" ? "success" : "danger"}
                    >
                      {log.status === "success" ? "成功" : "失败"}
                    </Chip>
                  </td>
                  <td className="px-4 py-3 text-right text-xs text-blue-400">
                    查看详情 →
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </Card>
      )}

      {logs.data && logs.data.total > 0 ? (
        <div className="mt-4 flex items-center justify-center gap-2 text-sm">
          <Button
            size="sm"
            variant="tertiary"
            isDisabled={page <= 1}
            onPress={() => setPage((p) => Math.max(1, p - 1))}
          >
            上一页
          </Button>
          <span className="text-white/50">
            {page} / {totalPages}
          </span>
          <Button
            size="sm"
            variant="tertiary"
            isDisabled={page >= totalPages}
            onPress={() => setPage((p) => p + 1)}
          >
            下一页
          </Button>
        </div>
      ) : null}

      <Drawer isOpen={!!active} onOpenChange={(o) => !o && setActive(null)}>
        <Drawer.Backdrop>
          <Drawer.Content placement="right">
            <Drawer.Dialog>
              <Drawer.Header>
                <Drawer.Heading>
                  {active ? `日志详情 · ${formatTimestamp(active.ts)}` : "日志详情"}
                </Drawer.Heading>
              </Drawer.Header>
              <Drawer.Body className="space-y-4 overflow-y-auto">
                {detailError ? (
                  <ErrorBanner message={detailError} />
                ) : !detail || detail.id !== active?.id ? (
                  <div className="space-y-2">
                    <div className="h-20 rounded bg-white/5 animate-pulse" />
                    <div className="h-20 rounded bg-white/5 animate-pulse" />
                  </div>
                ) : (
                  <>
                    <Section title="概览">
                      <Grid>
                        <KV label="状态">
                          <Chip
                            size="sm"
                            variant="soft"
                            color={detail.status === "success" ? "success" : "danger"}
                          >
                            {detail.status === "success" ? "成功" : "失败"}
                          </Chip>
                        </KV>
                        <KV label="HTTP 状态">
                          {detail.response_status ?? "—"}
                        </KV>
                        <KV label="来源">{detail.provider}</KV>
                        <KV label="模型">{detail.model ?? "—"}</KV>
                        <KV label="尺寸">{detail.size ?? "—"}</KV>
                        <KV label="种子">{detail.seed ?? "—"}</KV>
                        <KV label="耗时">{formatLatency(detail.latency_ms)}</KV>
                        <KV label="密钥">{detail.key_name ?? "—"}</KV>
                      </Grid>
                    </Section>

                    <Section title="提示词">
                      <div className="rounded bg-black/30 p-3 text-sm whitespace-pre-wrap break-words text-white/80">
                        {detail.prompt || "—"}
                      </div>
                    </Section>

                    {detail.error ? (
                      <Section title="错误信息">
                        <div className="rounded border border-rose-500/30 bg-rose-500/10 p-3 text-sm text-rose-200">
                          {detail.error}
                        </div>
                      </Section>
                    ) : null}

                    <Section title="请求体">
                      <pre className="max-h-72 overflow-auto rounded bg-black/40 p-3 text-xs text-white/80">
                        {tryPrettyJson(detail.request_body) || "—"}
                      </pre>
                    </Section>

                    {detail.image_urls && detail.image_urls.length ? (
                      <Section title={`生成图片 (${detail.image_urls.length})`}>
                        <div className="grid grid-cols-2 gap-3">
                          {detail.image_urls.map((src, i) => (
                            <a
                              key={i}
                              href={backendImageUrl(src)}
                              target="_blank"
                              rel="noopener noreferrer"
                              className="block"
                            >
                              {/* eslint-disable-next-line @next/next/no-img-element */}
                              <img
                                src={backendImageUrl(src)}
                                alt={`生成图 ${i + 1}`}
                                className="w-full rounded border border-white/10"
                              />
                            </a>
                          ))}
                        </div>
                      </Section>
                    ) : null}
                  </>
                )}
              </Drawer.Body>
              <Drawer.Footer>
                <Button variant="tertiary" onPress={() => setActive(null)}>
                  关闭
                </Button>
              </Drawer.Footer>
            </Drawer.Dialog>
          </Drawer.Content>
        </Drawer.Backdrop>
      </Drawer>

      <ConfirmModal
        isOpen={clearOpen}
        onOpenChange={setClearOpen}
        title="清空所有日志"
        description="此操作将永久删除所有日志记录，无法恢复。"
        confirmLabel="清空"
        danger
        onConfirm={async () => {
          await logsApi.clear();
          setPage(1);
          await logs.refresh();
        }}
      />
    </div>
  );
}

function Section({
  title,
  children,
}: {
  title: string;
  children: React.ReactNode;
}) {
  return (
    <section>
      <h4 className="mb-2 text-xs font-semibold uppercase tracking-wide text-white/40">
        {title}
      </h4>
      {children}
    </section>
  );
}

function Grid({ children }: { children: React.ReactNode }) {
  return <div className="grid grid-cols-2 gap-2 text-sm">{children}</div>;
}

function KV({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-2 rounded bg-black/20 px-3 py-2">
      <span className="text-xs text-white/50">{label}</span>
      <span className="text-right text-white/80">{children}</span>
    </div>
  );
}