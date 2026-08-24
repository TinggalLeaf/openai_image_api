"use client";

import { Suspense, useCallback, useEffect, useMemo, useState } from "react";
import { useRouter, useSearchParams } from "next/navigation";
import Link from "next/link";
import { Button, Chip, Disclosure, ScrollShadow, Switch } from "@heroui/react";
import {
  modelsApi,
  type ModelKind,
  type ModelMapping,
  type ModelRecord,
} from "@/lib/api";
import { Card, ErrorBanner, PageHeader } from "@/components/ui-helpers";
import { formatTimestamp } from "@/lib/utils";
import { Input } from "@/components/ui-form";

const MAPPING_FIELDS: Array<{
  key: keyof ModelMapping;
  label: string;
  placeholder: string;
}> = [
  { key: "prompt", label: "提示词节点", placeholder: "如 6.inputs.text" },
  { key: "negative_prompt", label: "反向提示词", placeholder: "如 7.inputs.text" },
  { key: "image", label: "图片输入节点（LoadImage）", placeholder: "如 10.inputs.image，留空表示不支持图片输入" },
  { key: "width", label: "宽度", placeholder: "如 5.inputs.width" },
  { key: "height", label: "高度", placeholder: "如 5.inputs.height" },
  { key: "seed", label: "种子", placeholder: "如 3.inputs.seed" },
  { key: "steps", label: "步数", placeholder: "如 3.inputs.steps" },
  { key: "cfg", label: "CFG", placeholder: "如 3.inputs.cfg" },
  { key: "sampler", label: "采样器", placeholder: "如 3.inputs.sampler_name" },
  { key: "scheduler", label: "调度器", placeholder: "如 3.inputs.scheduler" },
  { key: "batch_size", label: "批大小", placeholder: "如 3.inputs.batch_size" },
];

const KIND_LABEL: Record<ModelKind | string, string> = {
  t2i: "文生图",
  i2i: "图生图",
  edit: "编辑",
  audio: "音频",
  video: "视频",
  vision: "视觉",
  other: "其他",
};

const KIND_CHIP: Record<string, { variant: "primary" | "secondary" | "soft"; color: "default" | "accent" | "warning" }> = {
  t2i: { variant: "primary", color: "default" },
  i2i: { variant: "secondary", color: "default" },
  edit: { variant: "soft", color: "warning" },
  audio: { variant: "soft", color: "default" },
  video: { variant: "soft", color: "default" },
  vision: { variant: "soft", color: "default" },
  other: { variant: "soft", color: "default" },
};

export default function ModelDetailPage() {
  return (
    <Suspense
      fallback={
        <div>
          <PageHeader title="模型详情" />
          <div className="h-24 rounded-xl bg-[var(--surface-2)] animate-pulse" />
        </div>
      }
    >
      <ModelDetailInner />
    </Suspense>
  );
}

function ModelDetailInner() {
  const router = useRouter();
  const params = useSearchParams();
  const id = params.get("id") || "";
  const [model, setModel] = useState<ModelRecord | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [mapping, setMapping] = useState<ModelMapping | null>(null);
  const [enabled, setEnabled] = useState(true);
  const [saving, setSaving] = useState(false);
  const [saveMsg, setSaveMsg] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    if (!id) return;
    try {
      const data = await modelsApi.get(id);
      setModel(data);
      setMapping({ ...data.mapping, image: data.mapping.image ?? "" });
      setEnabled(data.enabled);
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : "加载失败");
      setModel(null);
    }
  }, [id]);

  useEffect(() => {
    let cancelled = false;
    // Defer with queueMicrotask to avoid calling setState synchronously
    // in the effect body (`react-hooks/set-state-in-effect`).
    queueMicrotask(() => {
      if (cancelled) return;
      if (!id) {
        setError("缺少模型 id 参数");
        return;
      }
      void (async () => {
        try {
          const data = await modelsApi.get(id);
          if (cancelled) return;
          setModel(data);
          setMapping({ ...data.mapping, image: data.mapping.image ?? "" });
          setEnabled(data.enabled);
        } catch (err) {
          if (cancelled) return;
          setError(err instanceof Error ? err.message : "加载失败");
        }
      })();
    });
    return () => {
      cancelled = true;
    };
  }, [id]);

  const workflowPretty = useMemo(() => {
    if (!model) return "";
    try {
      return JSON.stringify(model.workflow, null, 2);
    } catch {
      return "";
    }
  }, [model]);

  const dirty = useMemo(() => {
    if (!model || !mapping) return false;
    if (enabled !== model.enabled) return true;
    for (const f of MAPPING_FIELDS) {
      if ((mapping[f.key] ?? "") !== (model.mapping[f.key] ?? "")) return true;
    }
    return false;
  }, [model, mapping, enabled]);

  const save = async () => {
    if (!model || !mapping) return;
    setSaving(true);
    setSaveMsg(null);
    try {
      const updated = await modelsApi.update(model.id, {
        enabled,
        mapping,
      });
      setModel(updated);
      setMapping({ ...updated.mapping, image: updated.mapping.image ?? "" });
      setEnabled(updated.enabled);
      setSaveMsg("已保存");
    } catch (err) {
      setSaveMsg(err instanceof Error ? err.message : "保存失败");
    } finally {
      setSaving(false);
    }
  };

  if (!id) {
    return (
      <div>
        <PageHeader title="模型详情" />
        <ErrorBanner message="缺少模型 id 参数，请从模型列表进入。" />
        <div className="mt-4">
          <Button variant="tertiary" onPress={() => router.push("/models")}>
            返回列表
          </Button>
        </div>
      </div>
    );
  }

  if (error) {
    return (
      <div>
        <PageHeader title="模型详情" />
        <ErrorBanner message={error} />
        <div className="mt-4 flex gap-2">
          <Button variant="tertiary" onPress={() => router.push("/models")}>
            返回列表
          </Button>
          <Button variant="secondary" onPress={refresh}>
            重试
          </Button>
        </div>
      </div>
    );
  }

  if (!model || !mapping) {
    return (
      <div>
        <PageHeader title="模型详情" />
        <div className="space-y-3">
          <div className="h-24 rounded-xl bg-[var(--surface-2)] animate-pulse" />
          <div className="h-48 rounded-xl bg-[var(--surface-2)] animate-pulse" />
        </div>
      </div>
    );
  }

  const kind = model.kind ?? "other";
  const chip = KIND_CHIP[kind] ?? KIND_CHIP.other;

  return (
    <div>
      <PageHeader
        title={model.name}
        description={`模型 ID: ${model.id}`}
        actions={
          <>
            <Button
              variant="tertiary"
              onPress={() => router.push(`/playground?model=${model.id}`)}
            >
              前往测试
            </Button>
            <Button
              variant="primary"
              isDisabled={!dirty}
              onPress={save}
            >
              {saving ? "保存中…" : "保存"}
            </Button>
          </>
        }
      />

      {saveMsg ? (
        <div className="mb-4 rounded-lg border border-[var(--border)] bg-[var(--surface-2)] px-4 py-2 text-sm text-[var(--text)]">
          {saveMsg}
        </div>
      ) : null}

      <div className="grid grid-cols-1 gap-4 lg:grid-cols-3">
        <Card className="lg:col-span-2">
          <div className="mb-4 flex items-center justify-between">
            <h3 className="text-sm font-semibold">参数映射</h3>
            <div className="flex items-center gap-2 text-xs text-[var(--text-muted)]">
              <span>状态</span>
              <Switch size="sm" isSelected={enabled} onChange={setEnabled}>
                {enabled ? "已启用" : "已停用"}
              </Switch>
            </div>
          </div>
          <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
            {MAPPING_FIELDS.map((f) => (
              <Input
                key={f.key}
                label={f.label}
                placeholder={f.placeholder}
                value={mapping[f.key] ?? ""}
                onValueChange={(v) =>
                  setMapping({ ...mapping, [f.key]: v })
                }
              />
            ))}
          </div>
          <div className="mt-4 text-xs text-[var(--text-faint)]">
            每项填写 ComfyUI 工作流 JSON 中对应输入的节点路径，例如
            <span className="mx-1 rounded bg-[var(--surface-2)] px-1 font-mono">
              3.inputs.seed
            </span>
            表示 ID 为 3 的节点的 seed 输入。留空表示不参与替换。
          </div>
        </Card>

        <Card>
          <h3 className="text-sm font-semibold">基本信息</h3>
          <dl className="mt-3 space-y-2 text-sm">
            <Info label="类型">
              <Chip size="sm" variant={chip.variant} color={chip.color}>
                {KIND_LABEL[kind] ?? kind}
              </Chip>
            </Info>
            <Info label="创建时间">{formatTimestamp(model.created_at)}</Info>
            <Info label="更新时间">{formatTimestamp(model.updated_at)}</Info>
            <Info label="状态">
              <Chip size="sm" variant="soft" color={enabled ? "success" : "default"}>
                {enabled ? "已启用" : "已停用"}
              </Chip>
            </Info>
            <Info label="图片输入">
              {mapping.image ? (
                <span className="font-mono text-xs text-blue-400">{mapping.image}</span>
              ) : (
                <span className="text-[var(--text-faint)]">不支持</span>
              )}
            </Info>
          </dl>
          <div className="mt-4">
            <Link
              href="/models"
              className="text-xs text-blue-400 hover:underline"
            >
              ← 返回模型列表
            </Link>
          </div>
        </Card>
      </div>

      <Card className="mt-4">
        <Disclosure>
          <Disclosure.Heading>
            <Disclosure.Trigger className="flex w-full items-center justify-between rounded px-1 py-1 hover:bg-[var(--surface-2)]">
              <span className="text-sm font-semibold">
                工作流 JSON（只读）
              </span>
              <span className="text-xs text-[var(--text-faint)]">
                {workflowPretty.length.toLocaleString()} 字符
              </span>
            </Disclosure.Trigger>
          </Disclosure.Heading>
          <Disclosure.Content>
            <ScrollShadow className="mt-3 max-h-[480px] rounded-lg border border-[var(--border)] bg-[var(--surface-2)]">
              <pre className="p-4 text-xs leading-relaxed text-[var(--text)]">
                {workflowPretty}
              </pre>
            </ScrollShadow>
          </Disclosure.Content>
        </Disclosure>
      </Card>
    </div>
  );
}

function Info({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex items-center justify-between">
      <dt className="text-[var(--text-muted)]">{label}</dt>
      <dd className="text-[var(--text-strong)]">{children}</dd>
    </div>
  );
}
