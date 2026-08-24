"use client";

import { useEffect, useState } from "react";
import { Button } from "@heroui/react";
import {
  healthApi,
  settingsApi,
  storageApi,
  type SettingsResponse,
  type SettingsUpdateResponse,
  type StorageInfo,
} from "@/lib/api";
import { useAsync } from "@/lib/use-api";
import { Card, ErrorBanner, PageHeader } from "@/components/ui-helpers";
import { useHealth } from "@/components/health-provider";
import { Input, NumberInput } from "@/components/ui-form";
import { formatBytes, formatNumber } from "@/lib/utils";

const RESTART_LABELS: Record<string, string> = {
  listen_addr: "网关监听地址",
  data_dir: "数据目录",
  comfyui_url: "ComfyUI 地址",
  request_timeout_s: "请求超时",
  max_concurrent: "最大并发数",
};

export default function SettingsPage() {
  const { health, refresh: refreshHealth } = useHealth();
  const [form, setForm] = useState<SettingsResponse | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [saveMsg, setSaveMsg] = useState<string | null>(null);
  const [restartRequired, setRestartRequired] = useState<string[]>([]);
  const [checking, setChecking] = useState(false);
  const [checkResult, setCheckResult] = useState<string | null>(null);

  const storage = useAsync<StorageInfo>(
    () => storageApi.get(),
    [],
    { pollIntervalMs: 30_000 },
  );
  const [cleaning, setCleaning] = useState(false);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const data = await settingsApi.get();
        if (!cancelled) {
          setForm({
            listen_addr: data.listen_addr ?? "",
            data_dir: data.data_dir ?? "",
            comfyui_url: data.comfyui_url ?? "",
            request_timeout_s: data.request_timeout_s ?? 300,
            max_concurrent: data.max_concurrent ?? 1,
            image_retention_hours: data.image_retention_hours ?? 0,
            image_max_total_mb: data.image_max_total_mb ?? 0,
          });
        }
      } catch (err) {
        if (!cancelled)
          setLoadError(err instanceof Error ? err.message : "加载失败");
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  const save = async () => {
    if (!form) return;
    setSaving(true);
    setSaveMsg(null);
    setRestartRequired([]);
    try {
      const updated: SettingsUpdateResponse = await settingsApi.update({
        listen_addr: form.listen_addr,
        data_dir: form.data_dir,
        comfyui_url: form.comfyui_url,
        request_timeout_s: form.request_timeout_s,
        max_concurrent: form.max_concurrent,
        image_retention_hours: form.image_retention_hours,
        image_max_total_mb: form.image_max_total_mb,
      });
      setForm({
        ...form,
        ...updated,
      });
      setSaveMsg("已保存");
      if (Array.isArray(updated.restart_required) && updated.restart_required.length) {
        setRestartRequired(updated.restart_required);
      }
      await refreshHealth();
    } catch (err) {
      setSaveMsg(err instanceof Error ? err.message : "保存失败");
    } finally {
      setSaving(false);
    }
  };

  const checkConnection = async () => {
    setChecking(true);
    setCheckResult(null);
    try {
      const data = await healthApi.get();
      await refreshHealth();
      setCheckResult(
        data.comfyui_online
          ? "连接正常，ComfyUI 在线。"
          : "后端可达，但 ComfyUI 未连接。",
      );
    } catch (err) {
      setCheckResult(err instanceof Error ? err.message : "检查失败");
    } finally {
      setChecking(false);
    }
  };

  const cleanupNow = async () => {
    setCleaning(true);
    try {
      const res = await storageApi.cleanup();
      setSaveMsg(`已清理 ${res.deleted_count} 张图片，释放 ${formatBytes(res.freed_bytes)}`);
      await storage.refresh();
    } catch (err) {
      setSaveMsg(err instanceof Error ? err.message : "清理失败");
    } finally {
      setCleaning(false);
    }
  };

  return (
    <div>
      <PageHeader title="设置" description="配置网关运行参数与图片存储" />

      {loadError ? <ErrorBanner message={loadError} /> : null}

      <Card className="max-w-2xl">
        {!form ? (
          <div className="space-y-3">
            <div className="h-10 rounded bg-[var(--surface-2)] animate-pulse" />
            <div className="h-10 rounded bg-[var(--surface-2)] animate-pulse" />
          </div>
        ) : (
          <div className="space-y-4">
            {restartRequired.length > 0 ? (
              <div className="rounded-lg border border-amber-500/40 bg-amber-500/10 px-3 py-2 text-sm text-amber-200">
                以下配置需重启后端生效：
                <span className="ml-1 font-medium">
                  {restartRequired
                    .map((k) => RESTART_LABELS[k] ?? k)
                    .join("、")}
                </span>
              </div>
            ) : null}

            <Input
              label="网关监听地址"
              description="形如 0.0.0.0:8000；修改后需重启后端"
              value={form.listen_addr}
              onValueChange={(v) => setForm({ ...form, listen_addr: v })}
            />
            <Input
              label="数据目录"
              description="日志、生成图片等持久化文件所在目录"
              value={form.data_dir}
              onValueChange={(v) => setForm({ ...form, data_dir: v })}
            />
            <Input
              label="ComfyUI 地址"
              description="网关会连接该地址获取工作流状态与提交任务"
              value={form.comfyui_url}
              onValueChange={(v) => setForm({ ...form, comfyui_url: v })}
            />
            <NumberInput
              label="请求超时 (秒)"
              description="单次图像生成的最长等待时间"
              value={form.request_timeout_s}
              onValueChange={(v) => setForm({ ...form, request_timeout_s: v })}
              min={1}
              max={3600}
            />
            <NumberInput
              label="最大并发数"
              description="同时最多并行处理的生成任务数量，0 表示不限制"
              value={form.max_concurrent}
              onValueChange={(v) => setForm({ ...form, max_concurrent: v })}
              min={0}
              max={64}
            />
            <NumberInput
              label="图片保留时长 (小时)"
              description="生成图片超过该时长后自动清理；0 表示永久保留"
              value={form.image_retention_hours}
              onValueChange={(v) =>
                setForm({ ...form, image_retention_hours: v })
              }
              min={0}
              max={8760}
            />
            <NumberInput
              label="图片容量上限 (MB)"
              description="存储目录总占用超过该值时按时间清理最旧文件；0 表示不限制"
              value={form.image_max_total_mb}
              onValueChange={(v) =>
                setForm({ ...form, image_max_total_mb: v })
              }
              min={0}
              max={1048576}
            />

            <div className="flex flex-wrap items-center gap-2 pt-2">
              <Button variant="primary" onPress={save}>
                {saving ? "保存中…" : "保存"}
              </Button>
              <Button variant="tertiary" onPress={checkConnection}>
                {checking ? "检查中…" : "检查连接"}
              </Button>
              {saveMsg ? (
                <span className="text-xs text-[var(--text-muted)]">{saveMsg}</span>
              ) : null}
            </div>
            {checkResult ? (
              <div
                className={
                  "rounded-lg border px-3 py-2 text-sm " +
                  (checkResult.includes("正常")
                    ? "border-emerald-500/30 bg-emerald-500/10 text-emerald-200"
                    : "border-amber-500/30 bg-amber-500/10 text-amber-200")
                }
              >
                {checkResult}
              </div>
            ) : null}
            <div className="rounded-lg border border-[var(--border)] bg-[var(--surface-2)] px-3 py-2 text-xs text-[var(--text-muted)]">
              当前 ComfyUI 状态：
              <span
                className={
                  health?.comfyui_online
                    ? "ml-1 text-emerald-400"
                    : "ml-1 text-rose-400"
                }
              >
                {health?.comfyui_online ? "在线" : "离线"}
              </span>
              {health?.version ? (
                <span className="ml-2 text-[var(--text-faint)]">v{health.version}</span>
              ) : null}
            </div>
          </div>
        )}
      </Card>

      <div className="mt-6 max-w-2xl">
        <h2 className="mb-3 text-sm font-semibold">存储管理</h2>
        <Card>
          {storage.error ? (
            <ErrorBanner message={storage.error} />
          ) : !storage.data && storage.loading ? (
            <div className="space-y-2">
              <div className="h-6 rounded bg-[var(--surface-2)] animate-pulse" />
              <div className="h-6 rounded bg-[var(--surface-2)] animate-pulse" />
            </div>
          ) : !storage.data ? (
            <div className="text-xs text-[var(--text-faint)]">暂无数据</div>
          ) : (
            <div className="space-y-4">
              <dl className="grid grid-cols-2 gap-3 text-sm">
                <Stat label="文件数" value={formatNumber(storage.data.images_count)} />
                <Stat label="占用空间" value={formatBytes(storage.data.total_bytes)} />
                <Stat
                  label="保留时长"
                  value={
                    storage.data.retention_hours === 0
                      ? "永久"
                      : `${storage.data.retention_hours} 小时`
                  }
                />
                <Stat
                  label="容量上限"
                  value={
                    storage.data.max_total_mb === 0
                      ? "不限"
                      : `${storage.data.max_total_mb} MB`
                  }
                />
              </dl>
              {storage.data.dir ? (
                <div className="truncate rounded bg-[var(--surface-2)] px-3 py-2 text-xs text-[var(--text-muted)]" title={storage.data.dir}>
                  目录：{storage.data.dir}
                </div>
              ) : null}
              <div className="flex items-center gap-3 pt-2">
                <Button
                  variant="secondary"
                  onPress={cleanupNow}
                  isDisabled={cleaning}
                >
                  {cleaning ? "清理中…" : "立即清理"}
                </Button>
                <span className="text-xs text-[var(--text-faint)]">
                  会按保留时长与容量上限清理过期文件
                </span>
              </div>
            </div>
          )}
        </Card>
      </div>
    </div>
  );
}

function Stat({ label, value }: { label: string; value: React.ReactNode }) {
  return (
    <div className="rounded-lg border border-[var(--border)] bg-[var(--surface-2)] p-3">
      <div className="text-xs text-[var(--text-faint)]">{label}</div>
      <div className="mt-1 text-sm font-medium text-[var(--text-strong)]">{value}</div>
    </div>
  );
}
