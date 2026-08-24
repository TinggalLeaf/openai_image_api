// Centralised API client for the image generation gateway backend.
//
// Routing:
//   - Dev (`NODE_ENV === 'development'`): use `/backend` so Next.js
//     rewrites proxy to `http://127.0.0.1:8000`. The dev proxy has a
//     short default timeout (~30s) so long-running generation calls
//     (playground test, /v1/images/edits) bypass it via `directBackend`
//     and talk to :8000 directly (backend has CORS open in dev).
//   - Production (`next export` bundled into the Rust binary): the
//     static assets are served by the backend on the same origin, so
//     the API base is the empty string (relative URLs).
//
// Authentication:
//   - When the backend has `auth_required=true`, all `/admin/api/*`
//     endpoints (except `/admin/api/auth/status` and `/admin/api/login`)
//     require `Authorization: Bearer <token>`. The token is stored in
//     `localStorage["auth_token"]` and injected by the fetch wrapper
//     below. A 401 response clears the token and dispatches a
//     `auth:unauthorized` event so the React tree can route to /login.

const IS_DEV = process.env.NODE_ENV === "development";

// Dev proxy base; production is same-origin.
export const BACKEND_BASE = IS_DEV ? "/backend" : "";

export const DIRECT_BACKEND = (typeof window !== "undefined"
  ? `${window.location.protocol}//${window.location.hostname}:8000`
  : "http://127.0.0.1:8000");

export class ApiError extends Error {
  status: number;
  payload: unknown;
  constructor(message: string, status: number, payload?: unknown) {
    super(message);
    this.name = "ApiError";
    this.status = status;
    this.payload = payload;
  }
}

const TOKEN_KEY = "auth_token";

function getToken(): string | null {
  if (typeof window === "undefined") return null;
  try {
    return window.localStorage.getItem(TOKEN_KEY);
  } catch {
    return null;
  }
}

export function setAuthToken(token: string | null): void {
  if (typeof window === "undefined") return;
  try {
    if (token) window.localStorage.setItem(TOKEN_KEY, token);
    else window.localStorage.removeItem(TOKEN_KEY);
  } catch {
    // ignore
  }
  if (typeof window !== "undefined") {
    window.dispatchEvent(new CustomEvent("auth:changed"));
  }
}

export function getAuthToken(): string | null {
  return getToken();
}

interface RequestOptions {
  /** Bypass the dev `/backend` rewrite and talk to :8000 directly.
   *  Use for long-running generation requests where the dev proxy
   *  would otherwise time out. */
  direct?: boolean;
  /** Skip the auth Bearer header even if a token is stored. Useful
   *  for the auth/status and login endpoints themselves. */
  noAuth?: boolean;
  /** AbortSignal forwarded to fetch. */
  signal?: AbortSignal;
  /** Caller-provided timeout in milliseconds. When the timeout fires,
   *  the underlying request is aborted and an ApiError is thrown. */
  timeoutMs?: number;
}

async function request<T>(
  method: string,
  path: string,
  body?: unknown,
  options: RequestOptions = {},
): Promise<T> {
  const headers: Record<string, string> = {};
  let payload: BodyInit | undefined;
  if (body !== undefined) {
    if (body instanceof FormData) {
      payload = body;
    } else {
      headers["Content-Type"] = "application/json";
      payload = JSON.stringify(body);
    }
  }
  if (!options.noAuth) {
    const token = getToken();
    if (token) headers["Authorization"] = `Bearer ${token}`;
  }
  const base = options.direct ? DIRECT_BACKEND : BACKEND_BASE;
  const url = `${base}${path}`;

  // Compose signal: caller-provided + optional timeout
  const signals: AbortSignal[] = [];
  if (options.signal) signals.push(options.signal);
  let timeoutHandle: ReturnType<typeof setTimeout> | undefined;
  if (options.timeoutMs && options.timeoutMs > 0) {
    const ctrl = new AbortController();
    timeoutHandle = setTimeout(() => ctrl.abort(), options.timeoutMs);
    signals.push(ctrl.signal);
  }
  const signal = signals.length
    ? AbortSignal.any(signals)
    : undefined;

  let res: Response;
  try {
    res = await fetch(url, {
      method,
      headers,
      body: payload,
      ...(signal ? { signal } : {}),
    });
  } catch (err) {
    if (timeoutHandle) clearTimeout(timeoutHandle);
    throw err;
  } finally {
    if (timeoutHandle) clearTimeout(timeoutHandle);
  }

  const text = await res.text();
  let parsed: unknown = undefined;
  if (text) {
    try {
      parsed = JSON.parse(text);
    } catch {
      parsed = text;
    }
  }
  if (!res.ok) {
    // 401 from a protected endpoint → drop token, broadcast so the
    // React tree can re-route to /login.
    if (res.status === 401 && !options.noAuth && typeof window !== "undefined") {
      try {
        window.localStorage.removeItem(TOKEN_KEY);
      } catch {
        // ignore
      }
      window.dispatchEvent(new CustomEvent("auth:unauthorized"));
    }
    const message = extractErrorMessage(parsed) ?? `请求失败 (${res.status})`;
    throw new ApiError(message, res.status, parsed);
  }
  return parsed as T;
}

function extractErrorMessage(payload: unknown): string | undefined {
  if (!payload || typeof payload !== "object") return undefined;
  const obj = payload as Record<string, unknown>;
  if (typeof obj.message === "string") return obj.message;
  if (obj.error && typeof obj.error === "object") {
    const e = obj.error as Record<string, unknown>;
    if (typeof e.message === "string") return e.message;
  }
  if (typeof obj.error === "string") return obj.error;
  return undefined;
}

export const api = {
  get: <T>(path: string, options?: RequestOptions) =>
    request<T>("GET", path, undefined, options),
  post: <T>(path: string, body?: unknown, options?: RequestOptions) =>
    request<T>("POST", path, body ?? {}, options),
  put: <T>(path: string, body?: unknown, options?: RequestOptions) =>
    request<T>("PUT", path, body ?? {}, options),
  patch: <T>(path: string, body?: unknown, options?: RequestOptions) =>
    request<T>("PATCH", path, body ?? {}, options),
  del: <T>(path: string, options?: RequestOptions) =>
    request<T>("DELETE", path, undefined, options),
};

// --- Domain types ---------------------------------------------------------

export type ModelKind =
  | "t2i"
  | "i2i"
  | "edit"
  | "audio"
  | "video"
  | "vision"
  | "other";

export interface ModelMapping {
  prompt: string;
  negative_prompt: string;
  width: string;
  height: string;
  seed: string;
  steps: string;
  cfg: string;
  sampler: string;
  scheduler: string;
  batch_size: string;
  image: string;
}

export interface ModelRecord {
  id: string;
  name: string;
  kind: ModelKind;
  enabled: boolean;
  workflow: Record<string, unknown>;
  mapping: ModelMapping;
  created_at: number;
  updated_at: number;
}

export interface KeyRecord {
  id: string;
  key: string;
  name: string;
  enabled: boolean;
  created_at: number;
  last_used_at: number | null;
}

export interface LogRecord {
  id: string;
  ts: number;
  provider: string;
  key_name: string | null;
  model: string | null;
  prompt: string | null;
  size: string | null;
  seed: number | null;
  status: "success" | "error";
  error: string | null;
  latency_ms: number | null;
  image_urls: string[] | null;
  request_body: string | null;
  response_status: number | null;
}

export interface LogListResponse {
  items: LogRecord[];
  total: number;
  page: number;
  page_size: number;
}

export interface OverviewResponse {
  requests_total: number;
  requests_today: number;
  success_count: number;
  error_count: number;
  avg_latency_ms: number;
  models_count: number;
  keys_count: number;
  comfyui_online: boolean;
}

export interface HealthResponse {
  status: string;
  comfyui_online: boolean;
  version: string;
}

export interface SettingsResponse {
  listen_addr: string;
  data_dir: string;
  comfyui_url: string;
  request_timeout_s: number;
  max_concurrent: number;
  image_retention_hours: number;
  image_max_total_mb: number;
}

export interface SettingsUpdateResponse extends SettingsResponse {
  /** Field names that require a backend restart to take effect. */
  restart_required?: string[];
}

export interface TestGenerateBody {
  prompt: string;
  negative_prompt?: string;
  width?: number;
  height?: number;
  seed?: number;
  steps?: number;
  cfg?: number;
}

export interface TestGenerateResponse {
  images: string[];
  latency_ms: number;
  seed: number;
}

// --- Queue & storage ------------------------------------------------------

export interface QueueItem {
  model: string;
  prompt: string;
  since_ts: number;
}

export interface QueueStatus {
  max_concurrent: number;
  running: number;
  queued: number;
  running_items: QueueItem[];
  queued_items: QueueItem[];
}

export interface StorageInfo {
  dir: string;
  images_count: number;
  total_bytes: number;
  retention_hours: number;
  max_total_mb: number;
}

export interface StorageCleanupResponse {
  deleted_count: number;
  freed_bytes: number;
}

export interface ImportDirBody {
  path: string;
}

export interface ImportedModel {
  id: string;
  name: string;
  kind: string;
}

export interface SkippedModel {
  file: string;
  reason: string;
}

export interface ImportDirResponse {
  imported: ImportedModel[];
  skipped: SkippedModel[];
}

// --- Auth -----------------------------------------------------------------

export interface AuthStatus {
  auth_required: boolean;
}

export interface LoginResponse {
  token: string;
}

// --- Stats ----------------------------------------------------------------

export interface SeriesPoint {
  ts: number;
  total: number;
  success: number;
  error: number;
  avg_latency_ms: number;
}

export interface SeriesResponse {
  points: SeriesPoint[];
}

export interface KeyStats {
  key_id: string;
  key_name: string;
  total: number;
  success: number;
  error: number;
  avg_latency_ms: number;
  last_used_at: number | null;
}

export interface ModelStats {
  model_id: string;
  model_name: string;
  total: number;
  success: number;
  error: number;
  avg_latency_ms: number;
}

// --- Convenience wrappers --------------------------------------------------

export const authApi = {
  status: () => api.get<AuthStatus>("/admin/api/auth/status", { noAuth: true }),
  login: (password: string) =>
    api.post<LoginResponse>(
      "/admin/api/login",
      { password },
      { noAuth: true, timeoutMs: 10_000 },
    ),
};

export const healthApi = {
  get: () => api.get<HealthResponse>("/admin/api/health", { noAuth: true }),
};

export const overviewApi = {
  get: () => api.get<OverviewResponse>("/admin/api/overview"),
};

export const modelsApi = {
  list: () => api.get<ModelRecord[]>("/admin/api/models"),
  import: (body: unknown) => api.post<ModelRecord>("/admin/api/models/import", body),
  importDir: (body: ImportDirBody) =>
    api.post<ImportDirResponse>("/admin/api/models/import-dir", body),
  get: (id: string) => api.get<ModelRecord>(`/admin/api/models/${id}`),
  update: (id: string, body: Partial<ModelRecord>) =>
    api.put<ModelRecord>(`/admin/api/models/${id}`, body),
  remove: (id: string) => api.del<unknown>(`/admin/api/models/${id}`),
  test: (id: string, body: TestGenerateBody, signal?: AbortSignal) =>
    api.post<TestGenerateResponse>(
      `/admin/api/models/${id}/test`,
      body,
      // Bypass the dev proxy for long-running generations, and give
      // the request plenty of headroom in production as well.
      { direct: IS_DEV, signal, timeoutMs: 10 * 60_000 },
    ),
};

export const keysApi = {
  list: () => api.get<KeyRecord[]>("/admin/api/keys"),
  create: (name: string) => api.post<KeyRecord>("/admin/api/keys", { name }),
  patch: (id: string, enabled: boolean) =>
    api.patch<KeyRecord>(`/admin/api/keys/${id}`, { enabled }),
  remove: (id: string) => api.del<unknown>(`/admin/api/keys/${id}`),
};

export interface LogsQuery {
  page?: number;
  page_size?: number;
  status?: "success" | "error" | "";
  model?: string;
}

export const logsApi = {
  list: (q: LogsQuery = {}) => {
    const params = new URLSearchParams();
    if (q.page) params.set("page", String(q.page));
    if (q.page_size) params.set("page_size", String(q.page_size));
    if (q.status) params.set("status", q.status);
    if (q.model) params.set("model", q.model);
    const qs = params.toString();
    return api.get<LogListResponse>(`/admin/api/logs${qs ? `?${qs}` : ""}`);
  },
  get: (id: string) => api.get<LogRecord>(`/admin/api/logs/${id}`),
  clear: () => api.del<unknown>("/admin/api/logs"),
};

export const settingsApi = {
  get: () => api.get<SettingsResponse>("/admin/api/settings"),
  update: (body: Partial<SettingsResponse>) =>
    api.put<SettingsUpdateResponse>("/admin/api/settings", body),
};

export const queueApi = {
  get: () => api.get<QueueStatus>("/admin/api/queue"),
};

export const storageApi = {
  get: () => api.get<StorageInfo>("/admin/api/storage"),
  cleanup: () => api.post<StorageCleanupResponse>("/admin/api/storage/cleanup", {}),
};

export interface SeriesQuery {
  hours?: number;
  bucket?: "minute" | "hour" | "day";
  key_id?: string;
  model?: string;
}

export const statsApi = {
  series: (q: SeriesQuery = {}) => {
    const params = new URLSearchParams();
    if (q.hours) params.set("hours", String(q.hours));
    if (q.bucket) params.set("bucket", q.bucket);
    if (q.key_id) params.set("key_id", q.key_id);
    if (q.model) params.set("model", q.model);
    const qs = params.toString();
    return api.get<SeriesResponse>(`/admin/api/stats/series${qs ? `?${qs}` : ""}`);
  },
  byKey: () => api.get<KeyStats[]>("/admin/api/stats/keys"),
  byModel: () => api.get<ModelStats[]>("/admin/api/stats/models"),
};

// --- OpenAI-compatible images/edits ---------------------------------------

export interface ImagesEditsOptions {
  model: string;
  prompt: string;
  file: File;
  n?: number;
  size?: string;
}

export interface ImagesEditResponse {
  created: number;
  data: Array<{ url?: string; b64_json?: string }>;
}

/**
 * Submit a multipart image-edit request to the OpenAI-compatible
 * `/v1/images/edits` endpoint. The request is long-running (image
 * generation can take 30–90s+) so we bypass the Next.js dev proxy
 * even when the page is served from :3200, talking to :8000 directly
 * via the open CORS channel.
 */
export function editImage(opts: ImagesEditsOptions): Promise<ImagesEditResponse> {
  const form = new FormData();
  form.append("image", opts.file);
  form.append("prompt", opts.prompt);
  form.append("model", opts.model);
  if (opts.n !== undefined) form.append("n", String(opts.n));
  if (opts.size) form.append("size", opts.size);
  return request<ImagesEditResponse>(
    "POST",
    "/v1/images/edits",
    form,
    { direct: IS_DEV, timeoutMs: 10 * 60_000 },
  );
}

// Image URL helper: backend returns paths like "/files/images/xxx.png".
// The browser should fetch them through the rewrite in dev, or directly
// (same-origin) in production.
export function backendImageUrl(path: string): string {
  if (!path) return "";
  if (path.startsWith("http://") || path.startsWith("https://")) return path;
  if (path.startsWith("/")) return `${BACKEND_BASE}${path}`;
  return `${BACKEND_BASE}/${path}`;
}
