"use client";

import { useState } from "react";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { Button, Chip, Modal, Switch } from "@heroui/react";
import {
  modelsApi,
  type ImportedModel,
  type ImportDirResponse,
  type ModelKind,
  type ModelRecord,
  type SkippedModel,
} from "@/lib/api";
import { useAsync } from "@/lib/use-api";
import { Card, EmptyState, ErrorBanner, PageHeader } from "@/components/ui-helpers";
import { ConfirmModal } from "@/components/confirm-modal";
import { formatTimestamp } from "@/lib/utils";
import { Input, TextArea } from "@/components/ui-form";

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

export default function ModelsPage() {
  const router = useRouter();
  const models = useAsync<ModelRecord[]>(() => modelsApi.list(), []);
  const [importOpen, setImportOpen] = useState(false);
  const [importDirOpen, setImportDirOpen] = useState(false);
  const [deleteTarget, setDeleteTarget] = useState<ModelRecord | null>(null);

  const toggleEnabled = async (m: ModelRecord, enabled: boolean) => {
    try {
      await modelsApi.update(m.id, { enabled });
      await models.refresh();
    } catch (err) {
      alert(err instanceof Error ? err.message : "更新失败");
    }
  };

  return (
    <div>
      <PageHeader
        title="模型管理"
        description="导入 ComfyUI 工作流并配置参数映射，供生成请求调用"
        actions={
          <div className="flex flex-wrap gap-2">
            <Button variant="tertiary" onPress={() => setImportDirOpen(true)}>
              从目录批量导入
            </Button>
            <Button variant="primary" onPress={() => setImportOpen(true)}>
              导入工作流
            </Button>
          </div>
        }
      />

      {models.error ? <ErrorBanner message={models.error} /> : null}

      {models.loading && !models.data ? (
        <div className="grid grid-cols-1 gap-4 md:grid-cols-2 xl:grid-cols-3">
          {Array.from({ length: 3 }).map((_, i) => (
            <div key={i} className="h-40 rounded-xl bg-white/[0.03] animate-pulse" />
          ))}
        </div>
      ) : !models.data?.length ? (
        <Card>
          <EmptyState message="暂无模型，点击「导入工作流」开始添加" />
        </Card>
      ) : (
        <div className="grid grid-cols-1 gap-4 md:grid-cols-2 xl:grid-cols-3">
          {models.data.map((m) => (
            <Card key={m.id}>
              <div className="flex items-start justify-between gap-3">
                <div className="min-w-0">
                  <Link
                    href={`/models/detail?id=${encodeURIComponent(m.id)}`}
                    className="block truncate text-base font-semibold text-[var(--text-strong)] hover:text-blue-400"
                  >
                    {m.name}
                  </Link>
                  <div className="mt-1 text-xs text-[var(--text-faint)]">{m.id}</div>
                </div>
                <div className="flex shrink-0 flex-col items-end gap-1">
                  <Chip
                    size="sm"
                    variant="soft"
                    color={m.enabled ? "success" : "default"}
                  >
                    {m.enabled ? "启用" : "停用"}
                  </Chip>
                  <KindChip kind={m.kind} />
                </div>
              </div>

              {m.mapping?.image ? (
                <div className="mt-3 inline-flex items-center gap-1 rounded-md border border-blue-500/30 bg-blue-500/10 px-2 py-0.5 text-[11px] text-blue-300">
                  <ImageIcon className="h-3 w-3" />
                  支持图生图
                </div>
              ) : null}

              <dl className="mt-4 grid grid-cols-2 gap-y-2 text-xs">
                <Field label="提示词" value={m.mapping.prompt || "—"} />
                <Field label="种子" value={m.mapping.seed || "—"} />
                <Field label="步数" value={m.mapping.steps || "—"} />
                <Field label="CFG" value={m.mapping.cfg || "—"} />
              </dl>

              <div className="mt-4 flex items-center justify-between text-xs text-white/40">
                <span>更新于 {formatTimestamp(m.updated_at)}</span>
                <span>创建于 {formatTimestamp(m.created_at)}</span>
              </div>

              <div className="mt-4 flex items-center justify-between">
                <Switch
                  size="sm"
                  isSelected={m.enabled}
                  onChange={(v) => toggleEnabled(m, v)}
                >
                  <span className="text-xs text-white/70">
                    {m.enabled ? "已启用" : "已停用"}
                  </span>
                </Switch>
                <div className="flex gap-2">
                  <Button
                    size="sm"
                    variant="tertiary"
                    onPress={() => router.push(`/models/detail?id=${encodeURIComponent(m.id)}`)}
                  >
                    详情
                  </Button>
                  <Button
                    size="sm"
                    variant="tertiary"
                    onPress={() => setDeleteTarget(m)}
                  >
                    删除
                  </Button>
                </div>
              </div>
            </Card>
          ))}
        </div>
      )}

      <ImportWorkflowModal
        isOpen={importOpen}
        onOpenChange={setImportOpen}
        onSuccess={async () => {
          await models.refresh();
        }}
      />

      <ImportDirModal
        isOpen={importDirOpen}
        onOpenChange={setImportDirOpen}
        onSuccess={async () => {
          await models.refresh();
        }}
      />

      <ConfirmModal
        isOpen={!!deleteTarget}
        onOpenChange={(o) => !o && setDeleteTarget(null)}
        title="删除模型"
        description={
          <span>
            确定要删除模型
            <span className="font-semibold text-white">{deleteTarget?.name}</span>
            吗？此操作不可撤销。
          </span>
        }
        confirmLabel="删除"
        danger
        onConfirm={async () => {
          if (!deleteTarget) return;
          await modelsApi.remove(deleteTarget.id);
          await models.refresh();
        }}
      />
    </div>
  );
}

function KindChip({ kind }: { kind: string | undefined }) {
  const k = kind ?? "other";
  const chip = KIND_CHIP[k] ?? KIND_CHIP.other;
  const label = KIND_LABEL[k] ?? k;
  return (
    <Chip size="sm" variant={chip.variant} color={chip.color}>
      {label}
    </Chip>
  );
}

function ImageIcon(props: React.SVGProps<SVGSVGElement>) {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.8"
      strokeLinecap="round"
      strokeLinejoin="round"
      {...props}
    >
      <rect x="3" y="3" width="18" height="18" rx="2" />
      <circle cx="9" cy="9" r="2" />
      <path d="m21 15-5-5L5 21" />
    </svg>
  );
}

function Field({ label, value }: { label: string; value: string }) {
  return (
    <>
      <dt className="text-white/40">{label}</dt>
      <dd className="truncate font-mono text-white/80" title={value}>
        {value}
      </dd>
    </>
  );
}

function ImportWorkflowModal({
  isOpen,
  onOpenChange,
  onSuccess,
}: {
  isOpen: boolean;
  onOpenChange: (open: boolean) => void;
  onSuccess: () => Promise<void> | void;
}) {
  const [tab, setTab] = useState<"paste" | "file">("paste");
  const [text, setText] = useState("");
  const [name, setName] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [hint, setHint] = useState<string | null>(null);

  const reset = () => {
    setText("");
    setName("");
    setError(null);
    setHint(null);
    setTab("paste");
  };

  const handleFile = async (file: File | undefined) => {
    if (!file) return;
    try {
      const content = await file.text();
      setText(content);
      if (!name && file.name.endsWith(".json")) {
        setName(file.name.replace(/\.json$/i, ""));
      }
    } catch {
      setError("读取文件失败");
    }
  };

  const submit = async () => {
    setError(null);
    let parsed: unknown;
    try {
      parsed = JSON.parse(text);
    } catch {
      setError("JSON 解析失败，请检查内容");
      return;
    }
    setSubmitting(true);
    try {
      const body: Record<string, unknown> = { workflow: parsed };
      if (name.trim()) body.name = name.trim();
      const created = await modelsApi.import(body);
      const m = created.mapping;
      const mappedKeys = Object.entries(m)
        .filter(([, v]) => v)
        .map(([k]) => k);
      setHint(
        mappedKeys.length
          ? `已自动识别以下映射：${mappedKeys.join("、")}`
          : "已导入，但未识别到参数映射，可在模型详情页手动指定。",
      );
      await onSuccess();
      setTimeout(() => {
        onOpenChange(false);
        reset();
      }, 1500);
    } catch (err) {
      setError(err instanceof Error ? err.message : "导入失败");
    } finally {
      setSubmitting(false);
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
        <Modal.Container size="lg" placement="center">
          <Modal.Dialog>
            <Modal.Header>
              <Modal.Heading>导入 ComfyUI 工作流</Modal.Heading>
            </Modal.Header>
            <Modal.Body>
              <div className="mb-3 flex gap-2">
                <button
                  type="button"
                  onClick={() => setTab("paste")}
                  className={
                    "rounded-md px-3 py-1 text-xs font-medium transition-colors " +
                    (tab === "paste"
                      ? "bg-white/10 text-white"
                      : "text-white/50 hover:bg-white/5")
                  }
                >
                  粘贴 JSON
                </button>
                <button
                  type="button"
                  onClick={() => setTab("file")}
                  className={
                    "rounded-md px-3 py-1 text-xs font-medium transition-colors " +
                    (tab === "file"
                      ? "bg-white/10 text-white"
                      : "text-white/50 hover:bg-white/5")
                  }
                >
                  选择文件
                </button>
              </div>

              {tab === "paste" ? (
                <TextArea
                  label="工作流 JSON"
                  placeholder='{"3": {"class_type": "KSampler", ...}}'
                  rows={10}
                  value={text}
                  onValueChange={setText}
                />
              ) : (
                <div>
                  <label className="flex h-32 cursor-pointer flex-col items-center justify-center rounded-lg border border-dashed border-white/15 bg-black/20 text-sm text-white/60 hover:bg-black/30">
                    <input
                      type="file"
                      accept="application/json,.json"
                      className="hidden"
                      onChange={(e) => handleFile(e.target.files?.[0])}
                    />
                    {text ? (
                      <span className="text-white">已载入 JSON，点击替换</span>
                    ) : (
                      <span>点击选择本地 .json 文件</span>
                    )}
                  </label>
                  {text ? (
                    <pre className="mt-3 max-h-40 overflow-auto rounded bg-black/40 p-2 text-xs text-white/60">
                      {text.slice(0, 800)}
                      {text.length > 800 ? "…" : ""}
                    </pre>
                  ) : null}
                </div>
              )}

              <div className="mt-4">
                <Input
                  label="名称（可选）"
                  placeholder="不填则使用文件名或自动生成"
                  value={name}
                  onValueChange={setName}
                />
              </div>
              {error ? (
                <div className="mt-3 rounded border border-rose-500/30 bg-rose-500/10 px-3 py-2 text-sm text-rose-200">
                  {error}
                </div>
              ) : null}
              {hint ? (
                <div className="mt-3 rounded border border-emerald-500/30 bg-emerald-500/10 px-3 py-2 text-sm text-emerald-200">
                  {hint}
                </div>
              ) : null}
            </Modal.Body>
            <Modal.Footer>
              <Button
                variant="tertiary"
                onPress={() => onOpenChange(false)}
              >
                取消
              </Button>
              <Button
                variant="primary"
                isDisabled={!text || submitting}
                onPress={submit}
              >
                {submitting ? "导入中…" : "导入"}
              </Button>
            </Modal.Footer>
          </Modal.Dialog>
        </Modal.Container>
      </Modal.Backdrop>
    </Modal>
  );
}

function ImportDirModal({
  isOpen,
  onOpenChange,
  onSuccess,
}: {
  isOpen: boolean;
  onOpenChange: (open: boolean) => void;
  onSuccess: () => Promise<void> | void;
}) {
  const DEFAULT_PATH =
    "D:/ProgramData/comfyui/ComfyUI/ComfyUI/user/default/workflows";
  const [path, setPath] = useState(DEFAULT_PATH);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<ImportDirResponse | null>(null);

  const reset = () => {
    setPath(DEFAULT_PATH);
    setError(null);
    setResult(null);
  };

  const submit = async () => {
    setError(null);
    if (!path.trim()) {
      setError("请填写目录路径");
      return;
    }
    setSubmitting(true);
    try {
      const res = await modelsApi.importDir({ path: path.trim() });
      setResult(res);
      await onSuccess();
    } catch (err) {
      setError(err instanceof Error ? err.message : "导入失败");
    } finally {
      setSubmitting(false);
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
        <Modal.Container size="lg" placement="center">
          <Modal.Dialog>
            <Modal.Header>
              <Modal.Heading>从目录批量导入</Modal.Heading>
            </Modal.Header>
            <Modal.Body>
              <p className="mb-3 text-xs text-white/60">
                扫描指定目录下所有 ComfyUI 工作流 JSON 文件，自动识别类型并写入模型库。
              </p>
              <Input
                label="工作流目录"
                placeholder={DEFAULT_PATH}
                value={path}
                onValueChange={setPath}
              />
              {error ? (
                <div className="mt-3 rounded border border-rose-500/30 bg-rose-500/10 px-3 py-2 text-sm text-rose-200">
                  {error}
                </div>
              ) : null}
              {result ? <ImportDirResult result={result} /> : null}
            </Modal.Body>
            <Modal.Footer>
              <Button variant="tertiary" onPress={() => onOpenChange(false)}>
                {result ? "完成" : "取消"}
              </Button>
              {!result ? (
                <Button
                  variant="primary"
                  isDisabled={!path.trim() || submitting}
                  onPress={submit}
                >
                  {submitting ? "扫描中…" : "开始导入"}
                </Button>
              ) : null}
            </Modal.Footer>
          </Modal.Dialog>
        </Modal.Container>
      </Modal.Backdrop>
    </Modal>
  );
}

function ImportDirResult({ result }: { result: ImportDirResponse }) {
  const imported: ImportedModel[] = result.imported ?? [];
  const skipped: SkippedModel[] = result.skipped ?? [];
  return (
    <div className="mt-4 space-y-3 text-sm">
      <div>
        <div className="mb-1.5 flex items-center justify-between">
          <span className="text-xs font-medium text-emerald-300">
            已导入 ({imported.length})
          </span>
        </div>
        {!imported.length ? (
          <div className="rounded border border-white/5 bg-black/20 px-3 py-2 text-xs text-white/40">
            无
          </div>
        ) : (
          <ul className="max-h-48 space-y-1 overflow-auto">
            {imported.map((m) => (
              <li
                key={m.id}
                className="flex items-center justify-between rounded border border-white/5 bg-black/20 px-3 py-1.5 text-xs"
              >
                <span className="truncate text-white/80">{m.name}</span>
                <KindChip kind={m.kind} />
              </li>
            ))}
          </ul>
        )}
      </div>
      <div>
        <div className="mb-1.5 flex items-center justify-between">
          <span className="text-xs font-medium text-white/50">
            已跳过 ({skipped.length})
          </span>
        </div>
        {!skipped.length ? (
          <div className="rounded border border-white/5 bg-black/20 px-3 py-2 text-xs text-white/40">
            无
          </div>
        ) : (
          <ul className="max-h-32 space-y-1 overflow-auto">
            {skipped.map((s, i) => (
              <li
                key={`${s.file}-${i}`}
                className="flex items-center justify-between rounded border border-white/5 bg-black/20 px-3 py-1.5 text-xs"
              >
                <span className="truncate font-mono text-white/60" title={s.file}>
                  {s.file}
                </span>
                <span className="ml-2 shrink-0 text-white/40">{s.reason}</span>
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}
