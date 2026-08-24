# OpenAI Image API — ComfyUI 多格式图像生成网关

把本地 ComfyUI 包装成多种主流图像生成 API 格式，附带 WebUI 管理面板。

- **后端 `image_core/`**：Rust（axum + sqlx/SQLite + tokio-tungstenite），高性能异步网关
- **前端 `image_webui/`**：Next.js 16 + React 19 + HeroUI v3 + Tailwind CSS 4 管理面板

## 快速开始

```bash
# 1. 启动 ComfyUI（默认 http://127.0.0.1:8188）

# 2. 启动后端（默认 0.0.0.0:8000）
cd image_core && cargo run --release

# 3. 启动前端（注意避开 Windows 保留端口段，用 3200）
cd image_webui && npm install && npx next dev -p 3200
```

打开 http://127.0.0.1:3200 进入管理面板。

## 对外 API（`Authorization: Bearer <sk-...>`）

| 格式 | 端点 |
|---|---|
| OpenAI DALL·E | `POST /v1/images/generations`、`POST /v1/images/edits`（图生图，multipart）、`POST /v1/images/variations`、`GET /v1/models` |
| 火山引擎 Seedream | `POST /volcengine/api/v3/images/generations` |
| SiliconFlow | `POST /siliconflow/v1/images/generations` |
| 智谱 CogView | `POST /zhipu/paas/v4/images/generations` |

示例：

```bash
curl -X POST http://127.0.0.1:8000/v1/images/generations \
  -H "Authorization: Bearer sk-xxx" \
  -H "Content-Type: application/json" \
  -d '{"model":"<模型ID>","prompt":"a cute cat","size":"1024x1024"}'
```

## 面板功能

- **仪表盘**：请求统计、成功率、平均延迟、ComfyUI 在线状态、队列状态、图片存储占用
- **模型管理**：导入 ComfyUI 工作流（粘贴/上传 JSON，自动识别 UI/API 两种格式；或从目录批量导入），自动探测 prompt/尺寸/种子/图片输入等参数映射，可手动编辑；按 kind（文生图/图生图/音频/视频/视觉）分类
- **API 密钥**：创建/启停/删除 `sk-` 密钥
- **请求日志**：筛选、分页、详情（含生成图片）
- **在线测试**：选模型填 prompt 直接出图；图生图模型支持上传参考图
- **设置**：ComfyUI 地址、超时、最大并发数、图片保留时长/容量上限、手动清理

## 进阶特性

- **并发队列**：`max_concurrent` 限制同时执行的 ComfyUI 任务，超额请求排队等待（`GET /admin/api/queue` 可观测）
- **图片自动清理**：按保留时长（`image_retention_hours`）与总容量（`image_max_total_mb`）双策略，后台每 10 分钟执行，也可手动触发
- **UI 格式工作流**：直接导入 ComfyUI 前端保存的 `nodes/links` 格式 JSON，自动转换为 API 格式（含 Reroute 解析、合并节点平铺字段回退探测）

数据存于 `image_core/data/`（SQLite + 生成的图片）。示例工作流在 `image_core/examples/`。
