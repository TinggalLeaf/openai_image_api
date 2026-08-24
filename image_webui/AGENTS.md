<!-- BEGIN:nextjs-agent-rules -->

# This is NOT the Next.js you know

This version has breaking changes — APIs, conventions, and file structure may all differ from your training data. Read the relevant guide in `node_modules/next/dist/docs/` (resolved from this file's directory; in monorepos the `next` package may not be visible from the repo root) before writing any code. Heed deprecation notices.

This block is written and re-added by `next dev` — verify at `node_modules/next/dist/server/lib/generate-agent-files.js`. Removing it from a diff only re-creates the uncommitted change; committing it with your work keeps the tree clean.

<!-- END:nextjs-agent-rules -->

## OpenAI Image Gateway — Admin WebUI

This is the management console for the OpenAI-compatible image generation
gateway. It is built as a **static export** (`output: 'export'`) and
embedded into the Rust backend binary; the production deployment serves
the static assets from the same origin as the admin API on port 8000.

### Stack

- Next.js `16.3.2` (App Router, Turbopack, no `src/` directory,
  `output: 'export'`, `images: { unoptimized: true }`).
- React `19.2.8`.
- Tailwind CSS v4 via `@tailwindcss/postcss`.
- [HeroUI v3](https://heroui.com) (`@heroui/react`) for compound UI
  components (Button, Chip, Modal, Drawer, Tabs, Switch, Disclosure,
  ScrollShadow). HeroUI v3 is *not* the API from v2 — read its docs
  before changing components.
- Recharts for the dashboard / stats charts.
- Native HTML form controls (`<input>`, `<select>`, `<textarea>`) for
  data entry, styled via small wrappers in `components/ui-form.tsx`.

### Dev vs. production routing

`lib/api.ts` switches its base URL on `process.env.NODE_ENV`:

- **Dev (`NODE_ENV === 'development'`)** — `BACKEND_BASE = "/backend"`,
  which Next.js rewrites to `http://127.0.0.1:8000`. The dev proxy has a
  short default timeout (~30s), so long generation calls
  (`modelsApi.test`, `editImage`) bypass it via `directBackend` and
  talk to `:8000` directly through CORS.
- **Production (static export served from the Rust binary)** —
  `BACKEND_BASE = ""`. All API and image URLs become relative, so the
  browser talks to the same origin that served the page.

### Authentication

When the backend reports `auth_required: true`, every `/admin/api/*`
endpoint (except `/admin/api/auth/status` and `/admin/api/login`)
expects `Authorization: Bearer <token>`.

- The token is stored in `localStorage["auth_token"]` and injected by
  `lib/api.ts`'s fetch wrapper.
- Any 401 clears the token and dispatches a `auth:unauthorized`
  window event so `<AuthGuard>` can swap to `/login`.
- `<AuthGuard>` is mounted in the `(authed)` route group's layout; the
  login page lives outside the group so it can render without the
  AppShell.

### Light / dark theme

A boot-time inline `<script>` reads `localStorage["admin_theme"]` before
paint to avoid a flash. After hydration, `<ThemeProvider>` mirrors it
onto `<html class="dark">` and persists subsequent changes. `globals.css`
exposes theme tokens (`--background`, `--surface-1..3`, `--text-strong`,
…) and overrides Tailwind utilities like `bg-black/30` and `text-white/60`
when `html:not(.dark)` so components stay readable in light mode without
rewriting every file.

### App structure

```
app/
├── layout.tsx              Root layout, ThemeProvider + AuthProvider
│                           + HealthProvider (no AppShell — that's per-group)
├── globals.css             Tailwind + HeroUI imports + theme tokens
├── login/page.tsx          /login (outside the authed group)
└── (authed)/               Route group: AppShell + AuthGuard
    ├── layout.tsx          AppShell + AuthGuard wrapper
    ├── page.tsx            Dashboard (/)
    ├── models/
    │   ├── page.tsx        Model list (/models)
    │   └── detail/page.tsx Model detail (/models/detail?id=…)
    ├── keys/page.tsx       API keys (/keys)
    ├── logs/page.tsx       Request logs (/logs)
    ├── stats/page.tsx      Statistics (/stats)
    ├── playground/page.tsx Online test (/playground)
    └── settings/page.tsx   Settings (/settings)
components/
├── app-shell.tsx           Sidebar + topbar layout (mobile Drawer)
├── health-provider.tsx     Polls /admin/api/health every 30s
├── auth-provider.tsx       Tracks backend auth posture + token
├── auth-guard.tsx          Redirects unauthenticated traffic to /login
├── theme-provider.tsx      Light/dark toggle + persistence
├── charts.tsx              Recharts wrappers (area / line / pie)
├── confirm-modal.tsx       Generic yes/no dialog
├── ui-form.tsx             Input/TextArea/NumberInput/NativeSelect
└── ui-helpers.tsx          Card / PageHeader / EmptyState / ErrorBanner
lib/
├── api.ts                  Typed fetch client + domain types
├── use-api.ts              Tiny async data hook with optional polling
└── utils.ts                Timestamp / latency / masking helpers
```

### Running the UI

1. Make sure the Rust backend is up on `127.0.0.1:8000`.
2. Install dependencies if you have not already:
   `npm install`.
3. Start the dev server:
   `npm run dev`.
4. Open <http://localhost:3000>.

To point the UI at a different backend URL, set the `BACKEND_URL`
environment variable before running `next dev`. The rewrite is wired
in `next.config.ts`.

Production builds use `npm run build` — the static export lands in
`out/` and is embedded by the Rust binary at compile time.

### Conventions

- Pages are server components by default; interactive pages declare
  `"use client"` at the top.
- Use the helpers in `lib/api.ts` (`modelsApi`, `keysApi`, `logsApi`,
  `settingsApi`, `healthApi`, `overviewApi`, `queueApi`, `storageApi`,
  `statsApi`, `authApi`, `editImage`) — never call `fetch` directly.
- Backend image URLs come back as relative paths like
  `/files/images/<uuid>.png`. Use `backendImageUrl()` from `lib/api.ts`.
- Polling cadence lives in `HealthProvider` (30s) and
  `useAsync(..., { pollIntervalMs: 30_000 })`.
- Long-running calls (`modelsApi.test`, `editImage`) accept an
  `AbortSignal` and use a 10-minute timeout. They go through
  `direct: IS_DEV` so they bypass the Next dev proxy.
- HeroUI v3 Modal/Drawer are *compound* components
  (`<Modal.Backdrop><Modal.Container><Modal.Dialog>…`), not the
  flat `Modal.Content` from v2. `placement` lives on `Modal.Container`
  / `Drawer.Content`, not the root.
- `<Button>` does not accept a `title` HTML attribute. For tooltips on
  button-like affordances, use a plain `<button>` instead.
- The `<Chip>` component accepts `color` in `{accent, danger, default,
  success, warning}` and `variant` in `{primary, secondary, soft,
  tertiary}`.
- Avoid synchronous `setState` calls inside effect bodies (ESLint
  `react-hooks/set-state-in-effect`); defer with `queueMicrotask` if
  you really need an effect-driven initial fetch.
- The dynamic route `/models/[id]` does NOT exist — the static export
  cannot pre-render arbitrary IDs. Use `/models/detail?id=<uuid>`
  instead (handled with `useSearchParams` + `<Suspense>`).

### Functional checklist

- [x] Dashboard (`/`) — overview stats (requests, success rate,
  latency, models, keys, ComfyUI status, backend version); 近 24h
  请求量 area chart; 平均延迟 line chart; 模型用量 Top 5 pie;
  队列状态; 图片存储; 最近 10 条日志; 「全屏」button enters
  Fullscreen API + accelerates polling to 15s.
- [x] Model list (`/models`) — grid of cards with Kind chip, image-input
  badge, mapping summary, import/edit/delete, and the 「从目录批量
  导入」 modal.
- [x] Model detail (`/models/detail?id=…`) — editable mapping (incl.
  `image` field), basic info card, JSON viewer.
- [x] Settings (`/settings`) — listen_addr / data_dir / comfyui_url /
  request_timeout_s / max_concurrent / image_retention_hours /
  image_max_total_mb with a yellow warning banner when the PUT
  response includes `restart_required`; 「检查连接」 + 保存 +
  「立即清理」 storage management section.
- [x] Playground (`/playground`) — model picker, prompt / negative
  prompt / width / height / seed / steps / cfg; image upload for
  image-capable models routes through `editImage()`, otherwise the
  legacy `modelsApi.test`; loading label "生成中（可能在队列等待）…";
  both paths bypass the dev proxy via `direct: IS_DEV`.
- [x] API keys (`/keys`) — CRUD + per-row 「图表」 modal showing
  the key's 24h series + summary stats.
- [x] Logs (`/logs`) — paginated list with status / model filters.
- [x] Statistics (`/stats`) — range selector (1h/6h/24h/7d) driving
  `series`, plus a by-key and by-model table.
- [x] Login (`/login`) — centered card with password input; auto-
  redirects to dashboard when `auth_required=false`; on success
  stores token and routes back to `?next=…`.
- [x] AppShell — topbar theme toggle (sun/moon), exit button (only
  visible when auth_required), responsive Drawer for mobile nav.
