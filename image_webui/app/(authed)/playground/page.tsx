"use client";

import { Suspense, useEffect, useMemo, useState } from "react";
import { useSearchParams } from "next/navigation";
import { Button } from "@heroui/react";
import {
  backendImageUrl,
  editImage,
  modelsApi,
  type ModelRecord,
} from "@/lib/api";
import { useAsync } from "@/lib/use-api";
import {
  Card,
  EmptyState,
  ErrorBanner,
  PageHeader,
} from "@/components/ui-helpers";
import { formatLatency } from "@/lib/utils";
import { Input, NativeSelect, NumberInput, TextArea } from "@/components/ui-form";

interface FormState {
  model_id: string;
  prompt: string;
  negative_prompt: string;
  width: number;
  height: number;
  seed: number;
  steps: number;
  cfg: number;
}

const DEFAULT_FORM: FormState = {
  model_id: "",
  prompt: "",
  negative_prompt: "",
  width: 1024,
  height: 1024,
  seed: -1,
  steps: 20,
  cfg: 7,
};

type Mode = "t2i" | "i2i";

interface GenResult {
  urls: string[];
  latency_ms: number;
  seed: number;
  model_id: string;
  model_name: string;
  size?: string;
}

function PlaygroundInner() {
  const params = useSearchParams();
  const presetModel = params.get("model") ?? "";
  const models = useAsync<ModelRecord[]>(() => modelsApi.list(), []);
  const [form, setForm] = useState<FormState>(() => ({
    ...DEFAULT_FORM,
    model_id: presetModel,
  }));
  const [imageFile, setImageFile] = useState<File | null>(null);
  const [imagePreview, setImagePreview] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<GenResult | null>(null);

  const selected = models.data?.find((m) => m.id === form.model_id);
  const effectiveModelId =
    form.model_id ||
    presetModel ||
    models.data?.find((m) => m.enabled)?.id ||
    models.data?.[0]?.id ||
    "";

  // Whether the currently selected model accepts an image input.
  const supportsImageInput = !!selected?.mapping?.image;
  const mode: Mode = supportsImageInput && imageFile ? "i2i" : "t2i";

  // Build the size string expected by the OpenAI edits API.
  const sizeString = useMemo(() => `${form.width}x${form.height}`, [form.width, form.height]);

  // Revoke any pending blob URL when the component unmounts.
  useEffect(() => {
    return () => {
      if (imagePreview) URL.revokeObjectURL(imagePreview);
    };
    // We only want to revoke the final preview when unmounting.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const submit = async () => {
    if (!effectiveModelId) {
      setError("请先选择一个模型");
      return;
    }
    if (!form.prompt.trim()) {
      setError("请输入提示词");
      return;
    }
    if (mode === "i2i" && !imageFile) {
      setError("当前模型需要图片输入，请先上传图片");
      return;
    }
    setError(null);
    setSubmitting(true);
    setResult(null);
    const started = Date.now();
    try {
      if (mode === "i2i" && imageFile) {
        const data = await editImage({
          model: effectiveModelId,
          prompt: form.prompt,
          file: imageFile,
          n: 1,
          size: sizeString,
        });
        const urls = (data.data ?? [])
          .map((d) => d.url)
          .filter((u): u is string => typeof u === "string");
        setResult({
          urls,
          latency_ms: Date.now() - started,
          seed: 0,
          model_id: effectiveModelId,
          model_name: selected?.name ?? "",
          size: sizeString,
        });
      } else {
        const data = await modelsApi.test(effectiveModelId, {
          prompt: form.prompt,
          negative_prompt: form.negative_prompt || undefined,
          width: form.width,
          height: form.height,
          seed: form.seed,
          steps: form.steps,
          cfg: form.cfg,
        });
        setResult({
          urls: data.images,
          latency_ms: data.latency_ms,
          seed: data.seed,
          model_id: effectiveModelId,
          model_name: selected?.name ?? "",
        });
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : "生成失败");
    } finally {
      setSubmitting(false);
    }
  };

  const handleImageChange = (file: File | undefined) => {
    if (!file) {
      setImageFile(null);
      setImagePreview((prev) => {
        if (prev) URL.revokeObjectURL(prev);
        return null;
      });
      return;
    }
    if (!file.type.startsWith("image/")) {
      setError("请选择图片文件");
      return;
    }
    setError(null);
    setImageFile(file);
    setImagePreview((prev) => {
      if (prev) URL.revokeObjectURL(prev);
      return URL.createObjectURL(file);
    });
  };

  const modelOptions = models.data?.length
    ? models.data.map((m) => ({ value: m.id, label: m.name }))
    : [{ value: "", label: "请先导入模型" }];

  return (
    <div>
      <PageHeader
        title="在线测试"
        description="选择模型并发起一次生成调用，实时查看返回的图片与耗时"
      />

      <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
        <Card>
          <h3 className="mb-4 text-sm font-semibold text-white">请求参数</h3>
          <div className="space-y-4">
            <NativeSelect
              label="模型"
              value={form.model_id}
              onValueChange={(v) => setForm((f) => ({ ...f, model_id: v }))}
              disabled={!models.data?.length}
              options={modelOptions}
            />

            {supportsImageInput ? (
              <div>
                <label className="block">
                  <span className="mb-1.5 block text-xs font-medium text-white/70">
                    输入图片
                    <span className="ml-2 text-white/40">
                      （当前模型已配置图生图，留空将按文生图调用）
                    </span>
                  </span>
                  <label className="flex h-32 cursor-pointer flex-col items-center justify-center rounded-lg border border-dashed border-white/15 bg-black/20 text-sm text-white/60 hover:bg-black/30">
                    <input
                      type="file"
                      accept="image/*"
                      className="hidden"
                      onChange={(e) => handleImageChange(e.target.files?.[0])}
                    />
                    {imagePreview ? (
                      <div className="flex w-full items-center gap-3 px-4">
                        {/* eslint-disable-next-line @next/next/no-img-element */}
                        <img
                          src={imagePreview}
                          alt="输入预览"
                          className="h-20 w-20 rounded object-cover border border-white/10"
                        />
                        <div className="min-w-0 flex-1">
                          <div className="truncate text-white">{imageFile?.name}</div>
                          <div className="mt-1 text-xs text-white/40">点击替换</div>
                        </div>
                      </div>
                    ) : (
                      <span>点击选择本地图片</span>
                    )}
                  </label>
                </label>
                {imageFile ? (
                  <button
                    type="button"
                    onClick={() => setImageFile(null)}
                    className="mt-2 text-xs text-white/40 hover:text-white/60"
                  >
                    移除图片
                  </button>
                ) : null}
              </div>
            ) : null}

            <TextArea
              label="提示词"
              placeholder="一只在雨中漫步的猫，插画风格…"
              rows={4}
              value={form.prompt}
              onValueChange={(v) => setForm((f) => ({ ...f, prompt: v }))}
            />

            <TextArea
              label="反向提示词"
              placeholder="可选：低质量、模糊、扭曲…"
              rows={2}
              value={form.negative_prompt}
              onValueChange={(v) => setForm((f) => ({ ...f, negative_prompt: v }))}
            />

            <div className="grid grid-cols-2 gap-3">
              <NumberInput
                label="宽度"
                value={form.width}
                onValueChange={(v) => setForm((f) => ({ ...f, width: v }))}
                min={64}
                max={4096}
                step={64}
              />
              <NumberInput
                label="高度"
                value={form.height}
                onValueChange={(v) => setForm((f) => ({ ...f, height: v }))}
                min={64}
                max={4096}
                step={64}
              />
              <NumberInput
                label="种子 (-1 随机)"
                value={form.seed}
                onValueChange={(v) => setForm((f) => ({ ...f, seed: v }))}
                min={-1}
              />
              <NumberInput
                label="步数"
                value={form.steps}
                onValueChange={(v) => setForm((f) => ({ ...f, steps: v }))}
                min={1}
                max={100}
              />
              <NumberInput
                label="CFG"
                value={form.cfg}
                onValueChange={(v) => setForm((f) => ({ ...f, cfg: v }))}
                min={0}
                max={30}
                step={0.5}
              />
            </div>

            <Input
              label="模型节点数"
              description={
                selected
                  ? `已配置 ${Object.values(selected.mapping).filter(Boolean).length} 项参数映射`
                  : "请选择一个模型"
              }
              value={
                selected
                  ? `${selected.workflow ? Object.keys(selected.workflow).length : 0}`
                  : ""
              }
              readOnly
            />

            <Button
              variant="primary"
              fullWidth
              onPress={submit}
            >
              {submitting
                ? "生成中（可能在队列等待）…"
                : mode === "i2i"
                  ? "生成图片（图生图）"
                  : "生成图片"}
            </Button>
            {error ? <ErrorBanner message={error} /> : null}
          </div>
        </Card>

        <Card>
          <h3 className="mb-4 text-sm font-semibold text-white">结果</h3>
          {!result && !submitting ? (
            <EmptyState message="点击「生成图片」开始测试" />
          ) : submitting ? (
            <div className="space-y-3">
              <div className="aspect-square w-full animate-pulse rounded-lg bg-white/5" />
              <div className="h-3 w-32 animate-pulse rounded bg-white/5" />
            </div>
          ) : result ? (
            <div className="space-y-4">
              <div className="flex flex-wrap items-center gap-3 text-xs text-white/60">
                <span>耗时 {formatLatency(result.latency_ms)}</span>
                <span>·</span>
                {result.seed ? (
                  <>
                    <span>实际种子 {result.seed}</span>
                    <span>·</span>
                  </>
                ) : null}
                <span>模型 {result.model_name || result.model_id}</span>
                {result.size ? (
                  <>
                    <span>·</span>
                    <span>尺寸 {result.size}</span>
                  </>
                ) : null}
                <span>·</span>
                <span>{result.urls.length} 张</span>
              </div>
              <div
                className={
                  result.urls.length > 1
                    ? "grid grid-cols-2 gap-3"
                    : "space-y-3"
                }
              >
                {result.urls.map((src, i) => (
                  <a
                    key={i}
                    href={backendImageUrl(src)}
                    target="_blank"
                    rel="noopener noreferrer"
                  >
                    {/* eslint-disable-next-line @next/next/no-img-element */}
                    <img
                      src={backendImageUrl(src)}
                      alt={`生成图 ${i + 1}`}
                      className="w-full rounded-lg border border-white/10"
                    />
                  </a>
                ))}
              </div>
            </div>
          ) : null}
        </Card>
      </div>
    </div>
  );
}

export default function PlaygroundPage() {
  return (
    <Suspense fallback={<div className="text-sm text-white/50">加载中…</div>}>
      <PlaygroundInner />
    </Suspense>
  );
}
