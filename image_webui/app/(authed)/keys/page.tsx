"use client";

import { useState } from "react";
import { Button, Chip, Modal, Switch } from "@heroui/react";
import { keysApi, statsApi, type KeyRecord, type SeriesResponse } from "@/lib/api";
import { useAsync } from "@/lib/use-api";
import { Card, EmptyState, ErrorBanner, PageHeader } from "@/components/ui-helpers";
import { ConfirmModal } from "@/components/confirm-modal";
import { formatTimestamp, maskKey, formatLatency, formatNumber } from "@/lib/utils";
import { Input } from "@/components/ui-form";
import { RequestAreaChart, LatencyLineChart } from "@/components/charts";

export default function KeysPage() {
  const keys = useAsync<KeyRecord[]>(() => keysApi.list(), []);
  const [createOpen, setCreateOpen] = useState(false);
  const [deleteTarget, setDeleteTarget] = useState<KeyRecord | null>(null);
  const [chartTarget, setChartTarget] = useState<KeyRecord | null>(null);

  const toggle = async (k: KeyRecord, enabled: boolean) => {
    try {
      await keysApi.patch(k.id, enabled);
      await keys.refresh();
    } catch (err) {
      alert(err instanceof Error ? err.message : "更新失败");
    }
  };

  return (
    <div>
      <PageHeader
        title="API 密钥"
        description="管理可调用本网关的 Bearer Token"
        actions={
          <Button variant="primary" onPress={() => setCreateOpen(true)}>
            新建密钥
          </Button>
        }
      />

      {keys.error ? <ErrorBanner message={keys.error} /> : null}

      {keys.loading && !keys.data ? (
        <div className="space-y-2">
          {Array.from({ length: 3 }).map((_, i) => (
            <div key={i} className="h-12 rounded bg-[var(--surface-2)] animate-pulse" />
          ))}
        </div>
      ) : !keys.data?.length ? (
        <Card>
          <EmptyState message="暂无密钥" />
        </Card>
      ) : (
        <Card className="overflow-x-auto p-0">
          <table className="w-full text-sm">
            <thead>
              <tr className="text-left text-xs text-[var(--text-faint)] border-b border-[var(--border)]">
                <th className="px-4 py-3 font-medium">名称</th>
                <th className="px-4 py-3 font-medium">密钥</th>
                <th className="px-4 py-3 font-medium">状态</th>
                <th className="px-4 py-3 font-medium">最后使用</th>
                <th className="px-4 py-3 font-medium">创建时间</th>
                <th className="px-4 py-3 font-medium text-right">操作</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-[var(--border)]">
              {keys.data.map((k) => (
                <tr key={k.id} className="text-[var(--text)]">
                  <td className="px-4 py-3 font-medium">{k.name}</td>
                  <td className="px-4 py-3 font-mono text-xs">
                    {maskKey(k.key)}
                  </td>
                  <td className="px-4 py-3">
                    <Chip
                      size="sm"
                      variant="soft"
                      color={k.enabled ? "success" : "default"}
                    >
                      {k.enabled ? "启用" : "停用"}
                    </Chip>
                  </td>
                  <td className="px-4 py-3 text-xs text-[var(--text-muted)]">
                    {formatTimestamp(k.last_used_at)}
                  </td>
                  <td className="px-4 py-3 text-xs text-[var(--text-muted)]">
                    {formatTimestamp(k.created_at)}
                  </td>
                  <td className="px-4 py-3 text-right">
                    <div className="inline-flex items-center gap-3">
                      <Button
                        size="sm"
                        variant="tertiary"
                        onPress={() => setChartTarget(k)}
                      >
                        图表
                      </Button>
                      <Switch
                        size="sm"
                        isSelected={k.enabled}
                        onChange={(v) => toggle(k, v)}
                      />
                      <Button
                        size="sm"
                        variant="tertiary"
                        onPress={() => setDeleteTarget(k)}
                      >
                        删除
                      </Button>
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </Card>
      )}

      <CreateKeyModal
        isOpen={createOpen}
        onOpenChange={setCreateOpen}
        onSuccess={() => keys.refresh()}
      />

      <KeyChartModal
        keyRecord={chartTarget}
        onOpenChange={(o) => !o && setChartTarget(null)}
      />

      <ConfirmModal
        isOpen={!!deleteTarget}
        onOpenChange={(o) => !o && setDeleteTarget(null)}
        title="删除密钥"
        description={
          <span>
            确定删除密钥
            <span className="font-semibold">{deleteTarget?.name}</span>
            ？此操作不可撤销，依赖该密钥的客户端将立即失去访问权限。
          </span>
        }
        confirmLabel="删除"
        danger
        onConfirm={async () => {
          if (!deleteTarget) return;
          await keysApi.remove(deleteTarget.id);
          await keys.refresh();
        }}
      />
    </div>
  );
}

function KeyChartModal({
  keyRecord,
  onOpenChange,
}: {
  keyRecord: KeyRecord | null;
  onOpenChange: (open: boolean) => void;
}) {
  return (
    <Modal
      isOpen={!!keyRecord}
      onOpenChange={onOpenChange}
    >
      <Modal.Backdrop>
        <Modal.Container size="lg" placement="center">
          <Modal.Dialog>
            <Modal.Header>
              <Modal.Heading>
                {keyRecord ? `用量趋势 · ${keyRecord.name}` : "用量趋势"}
              </Modal.Heading>
            </Modal.Header>
            <Modal.Body>
              {keyRecord ? (
                <KeyChartBody keyId={keyRecord.id} />
              ) : null}
            </Modal.Body>
            <Modal.Footer>
              <Button variant="tertiary" onPress={() => onOpenChange(false)}>
                关闭
              </Button>
            </Modal.Footer>
          </Modal.Dialog>
        </Modal.Container>
      </Modal.Backdrop>
    </Modal>
  );
}

function KeyChartBody({ keyId }: { keyId: string }) {
  const series = useAsync<SeriesResponse>(
    () => statsApi.series({ hours: 24, bucket: "hour", key_id: keyId }),
    [keyId],
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
  const avgLatency =
    totals.total > 0
      ? points.reduce((s, p) => s + p.avg_latency_ms * p.total, 0) /
        Math.max(1, totals.total)
      : 0;
  const successRate =
    totals.total > 0
      ? Math.round((totals.success / totals.total) * 100)
      : null;

  return (
    <div className="space-y-4">
      {series.error ? (
        <ErrorBanner message={series.error} />
      ) : null}
      <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
        <Stat label="总请求" value={formatNumber(totals.total)} />
        <Stat
          label="成功率"
          value={successRate === null ? "—" : `${successRate}%`}
          subtitle={`${formatNumber(totals.success)} / ${formatNumber(totals.total)}`}
        />
        <Stat label="失败" value={formatNumber(totals.error)} />
        <Stat label="平均延迟" value={formatLatency(avgLatency)} />
      </div>
      <div>
        <div className="mb-1 text-xs font-medium text-[var(--text-muted)]">
          请求量（近 24 小时）
        </div>
        {series.loading && !series.data ? (
          <div className="h-[220px] animate-pulse rounded bg-[var(--surface-2)]" />
        ) : (
          <RequestAreaChart points={points} hours={24} />
        )}
      </div>
      <div>
        <div className="mb-1 text-xs font-medium text-[var(--text-muted)]">
          平均延迟（近 24 小时）
        </div>
        {series.loading && !series.data ? (
          <div className="h-[180px] animate-pulse rounded bg-[var(--surface-2)]" />
        ) : (
          <LatencyLineChart points={points} hours={24} />
        )}
      </div>
    </div>
  );
}

function Stat({
  label,
  value,
  subtitle,
}: {
  label: string;
  value: React.ReactNode;
  subtitle?: string;
}) {
  return (
    <div className="rounded-lg border border-[var(--border)] bg-[var(--surface-2)] p-3">
      <div className="text-xs text-[var(--text-faint)]">{label}</div>
      <div className="mt-1 text-base font-medium text-[var(--text-strong)]">
        {value}
      </div>
      {subtitle ? (
        <div className="mt-1 text-[11px] text-[var(--text-faint)]">{subtitle}</div>
      ) : null}
    </div>
  );
}

function CreateKeyModal({
  isOpen,
  onOpenChange,
  onSuccess,
}: {
  isOpen: boolean;
  onOpenChange: (open: boolean) => void;
  onSuccess: () => Promise<void> | void;
}) {
  const [name, setName] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [created, setCreated] = useState<KeyRecord | null>(null);
  const [copied, setCopied] = useState(false);

  const reset = () => {
    setName("");
    setError(null);
    setCreated(null);
    setCopied(false);
  };

  const submit = async () => {
    setError(null);
    if (!name.trim()) {
      setError("请输入名称");
      return;
    }
    setSubmitting(true);
    try {
      const k = await keysApi.create(name.trim());
      setCreated(k);
      await onSuccess();
    } catch (err) {
      setError(err instanceof Error ? err.message : "创建失败");
    } finally {
      setSubmitting(false);
    }
  };

  const copy = async () => {
    if (!created) return;
    try {
      await navigator.clipboard.writeText(created.key);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      // ignore
    }
  };

  return (
    <Modal
      isOpen={isOpen}
      onOpenChange={(o) => {
        onOpenChange(o);
        if (!o) reset();
      }}
    >
      <Modal.Backdrop>
        <Modal.Container placement="center">
          <Modal.Dialog>
            <Modal.Header>
              <Modal.Heading>新建 API 密钥</Modal.Heading>
            </Modal.Header>
            <Modal.Body>
              {!created ? (
                <>
                  <Input
                    label="名称"
                    placeholder="例如 prod-server-01"
                    value={name}
                    onValueChange={setName}
                  />
                  {error ? (
                    <div className="mt-3 rounded border border-rose-500/30 bg-rose-500/10 px-3 py-2 text-sm text-rose-200">
                      {error}
                    </div>
                  ) : null}
                </>
              ) : (
                <div className="space-y-3">
                  <div className="rounded-lg border border-amber-500/30 bg-amber-500/10 px-3 py-2 text-sm text-amber-200">
                    请立即保存以下密钥，它只会完整显示这一次。
                  </div>
                  <div className="rounded-lg border border-[var(--border)] bg-[var(--surface-2)] p-3">
                    <div className="mb-2 flex items-center justify-between">
                      <span className="text-xs text-[var(--text-muted)]">{created.name}</span>
                      <Button size="sm" variant="tertiary" onPress={copy}>
                        {copied ? "已复制" : "复制"}
                      </Button>
                    </div>
                    <code className="block break-all font-mono text-sm text-[var(--text-strong)]">
                      {created.key}
                    </code>
                  </div>
                </div>
              )}
            </Modal.Body>
            <Modal.Footer>
              <Button variant="tertiary" onPress={() => onOpenChange(false)}>
                {created ? "完成" : "取消"}
              </Button>
              {!created ? (
                <Button variant="primary" onPress={submit}>
                  {submitting ? "创建中…" : "创建"}
                </Button>
              ) : null}
            </Modal.Footer>
          </Modal.Dialog>
        </Modal.Container>
      </Modal.Backdrop>
    </Modal>
  );
}
