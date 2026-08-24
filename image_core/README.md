# image_core

A Rust backend that wraps a local ComfyUI instance behind four popular image-generation
HTTP API formats (OpenAI DALL·E, Volcengine Ark Seedream, SiliconFlow, Zhipu CogView) and
exposes an admin API for a local management panel.

The compiled binary embeds the bundled Next.js admin console as static assets, so a single
`image_core.exe` (or `image_core` on Unix) is everything you need to ship.

## Quick start

```bash
# First launch: a .env template is created next to the executable and the server starts.
cargo build
cargo run                      # listens on 0.0.0.0:8000
# or override env on the command line:
LISTEN_ADDR=127.0.0.1:9000 COMFYUI_URL=http://192.168.1.10:8188 ADMIN_PASSWORD=secret cargo run
```

The SQLite database file is created at `${DATA_DIR}/image_core.db` on first run.
Generated images are stored in `${DATA_DIR}/images/` and served at `GET /files/images/{name}`.

## Configuration (.env)

All knobs live in a `.env` file. The server looks for it in this order:

1. `<exe-dir>/.env` (next to the binary — preferred for releases)
2. `./.env` (cwd)
3. `../.env`, `../../.env` (parent directories — handy when running from `image_core/`)
4. `<repo-root>/.env` (relative to `CARGO_MANIFEST_DIR`)

If none is found, a starter template (with Chinese comments) is written to the
executable's directory so first-launch users have something to edit.

| Var                    | Default                  | Notes                                                |
|------------------------|--------------------------|------------------------------------------------------|
| `LISTEN_ADDR`          | `0.0.0.0:8000`           | restart_required on change                            |
| `COMFYUI_URL`          | `http://127.0.0.1:8188`  | hot-applied to /prompt & /ws                         |
| `REQUEST_TIMEOUT_S`    | `300`                    | per-request timeout, queue wait + execution         |
| `DATA_DIR`             | `./data`                 | restart_required on change (paths can't move mid-run) |
| `MAX_CONCURRENT`       | `2`                      | ComfyUI executions in parallel; hot-rebuilt          |
| `IMAGE_RETENTION_HOURS`| `72` (0 = forever)       | auto-deleted after this many hours                   |
| `IMAGE_MAX_TOTAL_MB`   | `1024` (0 = unlimited)   | oldest-first eviction once exceeded                  |
| `ADMIN_PASSWORD`       | `` (empty)               | non-empty enables admin-token auth                   |
| `RUST_LOG`             | `info,sqlx=warn`         | tracing-subscriber directive                          |

**Layering.** Values are looked up in this order, with the first non-null winning:

1. Baked-in defaults.
2. `.env` file (via `dotenvy`).
3. Process environment.
4. `settings` table (editable at runtime via `PUT /admin/api/settings`).

The settings table wins for any field it has a row for, so the admin panel always
overrides env defaults. `PUT` returns `{"ok": true, "restart_required": ["listen_addr", …]}`
for fields that need a process restart to take effect.

## Endpoints

### Public image generation (require `Authorization: Bearer sk-…`)

| Format       | Path                                                  |
|--------------|-------------------------------------------------------|
| OpenAI       | `POST /v1/images/generations` (and `/openai/v1/images/generations`) |
|              | `POST /v1/images/edits` (multipart — at least one `image`) |
|              | `POST /v1/images/variations` (multipart — single `image`) |
|              | `GET  /v1/models`                                     |
| Volcengine   | `POST /volcengine/api/v3/images/generations`          |
| SiliconFlow  | `POST /siliconflow/v1/images/generations`             |
| Zhipu        | `POST /zhipu/paas/v4/images/generations`              |

Request shape (every provider): `{model, prompt, [negative_prompt, width, height, steps, cfg, seed, batch_size, …]}`.
Each provider's wrapper additionally accepts the vendor-specific fields listed in the code
(`size`, `quality`, `response_format`, `guidance_scale`, `image_size`, `num_inference_steps`,
`user_id`, etc.).

`/v1/images/edits` and `/v1/images/variations` upload one or more images to ComfyUI
via `POST /upload/image` and wire them into the model's `LoadImage` (`mapping.image`)
input before execution. Returns OpenAI-shaped JSON. A model without an `image` mapping
returns `400 no_image_input`.

### Admin (`/admin/api/*`)

When `ADMIN_PASSWORD` is set, every `/admin/api/*` except `/login` and `/auth/status`
requires `Authorization: Bearer <token>`. Get a token by `POST /admin/api/login`
with `{"password": "..."}`.

- `GET    /admin/api/auth/status` → `{"auth_required": true|false}` (always public)
- `POST   /admin/api/login` → `{"token": "<64-hex>"}` on success; `401 wrong_password` otherwise
- `GET    /admin/api/health`
- `GET    /admin/api/overview`
- `GET    /admin/api/queue` — current concurrency snapshot
- `GET    /admin/api/storage` — image dir stats
- `POST   /admin/api/storage/cleanup` — manual retention/total-size sweep
- `GET    /admin/api/models`
- `POST   /admin/api/models/import` — body is either a raw ComfyUI workflow JSON
  (UI or API format; UI is auto-converted) or `{"name": "...", "workflow": {...}}`.
  Mapping and kind are auto-detected.
- `POST   /admin/api/models/import-dir` — `{"path": "..."}` walks a directory and
  imports every `*.json`. Names are filenames without `.json`. Skips on collision
  or conversion failure. Response: `{"imported":[{id,name,kind}], "skipped":[{file,reason}]}`.
- `GET    /admin/api/models/{id}`
- `PUT    /admin/api/models/{id}`
- `DELETE /admin/api/models/{id}`
- `POST   /admin/api/models/{id}/test`
- `GET    /admin/api/keys` · `POST /admin/api/keys` · `PATCH /admin/api/keys/{id}` · `DELETE /admin/api/keys/{id}`
- `GET    /admin/api/logs?page=&page_size=&status=&model=` · `DELETE /admin/api/logs` · `GET /admin/api/logs/{id}`
- `GET    /admin/api/settings` · `PUT /admin/api/settings` (accepts every field in
  the table; returns `{"ok":true,"restart_required":[...]}` when a restart-needed
  field changed)
- `GET    /admin/api/stats/series?hours=24&bucket=hour&key_id=&model=` — time-bucketed counts
- `GET    /admin/api/stats/keys` — per-key aggregates (incl. never-used keys at 0)
- `GET    /admin/api/stats/models` — per-model aggregates

### Static

- `GET /files/images/{filename}` — serves generated PNG/JPEG/etc.
- `GET /` — serves the bundled Next.js admin SPA. Any non-API, non-`/files` path
  falls back to `index.html` so client-side routing works.

## Workflow format support

The import endpoints accept both shapes:

1. **API format** — what ComfyUI's `/prompt` endpoint expects:
   `{ "<nodeId>": { "class_type": "...", "inputs": { ... } } }`.
2. **UI format** — what the ComfyUI frontend writes to disk:
   `{ "nodes": [...], "links": [[link_id, src_node, src_slot, dst_node, dst_slot, type], ...] }`.

UI-format files are auto-detected and converted via `workflow_convert::ui_to_api`.
Conversion handles:
- Widget arrays aligned to widget-marked inputs in declaration order.
- `{"value": x}` wrapper unwrapping for combo widgets.
- `Reroute` nodes are dropped but their links are followed to the real source
  (cycle-protected up to 32 hops).
- `MarkdownNote` / `Note` nodes are dropped — but links pointing at them
  produce an error rather than silently producing broken workflows.
- UUID `class_type` strings are preserved verbatim.

For "merged-node" workflows (single UUID mega-node with all sampler/CLIP/EmptyLatent
inputs flattened into one), `detect_mapping` falls back to scoring each node by
the number of recognized field names (text/seed/steps/cfg/sampler_name/scheduler/
width/height/batch_size/positive/negative) and picks the highest-scoring one.

## Model `kind` detection

`detect_kind` classifies an imported workflow as one of `audio`, `video`, `vision`,
`i2i`, `t2i`, `other`. Priority is `audio > video > vision > i2i > t2i > other`.
Token matching is word-boundary aware so e.g. `"wan"` doesn't match `"previewany"`.

## Concurrency queue

A single `tokio::Semaphore` (size = `max_concurrent`) gates every ComfyUI
execution. `GET /admin/api/queue` returns:
```json
{
  "max_concurrent": 2,
  "running": 0,
  "queued": 0,
  "running_items": [],
  "queued_items": []
}
```
The permit covers the entire request lifetime: queue wait + ComfyUI POST + WS/poll + image fetch.

## Image auto-cleanup

A background task scans `${DATA_DIR}/images/` every 10 minutes, deletes files whose
mtime is older than `IMAGE_RETENTION_HOURS`, and then evicts oldest-first until
total size is at or below `IMAGE_MAX_TOTAL_MB`. Every removal emits a tracing
log. The same logic is exposed as `POST /admin/api/storage/cleanup` for manual
triggers.

## Embedding the admin SPA

The admin console in `image_webui/` is built with `next build` (static export) and
the contents of `image_webui/out/` are embedded at compile time via `rust-embed`.
The build script (see `image_core/assets/.gitignore` and CI workflow) copies
`image_webui/out/` into `image_core/assets/` before running `cargo build`.

In dev / contributor mode the binary still builds even before the SPA has been
exported — `image_core/assets/index.html` ships a placeholder explaining the
state.

## Tests

```bash
cargo test
```

Covers:
- `workflow_convert::tests` — UI → API conversion (incl. real workflow files in
  `D:/ProgramData/comfyui/.../workflows/` when present; otherwise skipped).
- `mapping::tests` — auto-detect mapping and kind, plus `LoadImage` image-input
  detection and flat-field fallback for merged-node workflows.
- `cleanup::tests` — directory scan on a nonexistent path.
- `auth::tests` — constant-time password comparison.
- `embedded::tests` — embedded SPA index presence + SPA fallback + mime guessing.

## CI / Releases

`.github/workflows/build.yml` builds the binary on six targets (windows x86_64,
linux x86_64, linux aarch64 cross-compiled on ubuntu-22.04-arm, macos aarch64,
macos x86_64) on every push of a `v*` tag or manual trigger. The SPA is built
first via `npm ci && npm run build` inside `image_webui/`, the `out/` directory
is copied into `image_core/assets/`, then `cargo build --release` runs.
Single-file archives (with `image_core/README.md`) are uploaded as build
artifacts and attached to a GitHub Release (via `softprops/action-gh-release`).