# OpenAI Image API — ComfyUI 多格式图像生成网关

把本地 ComfyUI 包装成多种主流图像生成 API 格式（OpenAI DALL·E / 火山引擎 Seedream / SiliconFlow / 智谱 CogView），附带功能完整的 WebUI 管理面板。

- **后端 `image_core/`**：Rust（axum + sqlx/SQLite + tokio-tungstenite），高性能异步网关
- **前端 `image_webui/`**：Next.js 16 + React 19 + HeroUI v3 + Tailwind CSS 4 管理面板，可静态导出并**嵌入后端二进制**，实现单文件发布

## 快速开始

### 方式一：单文件可执行（推荐）

从 [Releases](../../releases) 下载对应平台的单文件，直接运行：

```bash
./image_core        # 首次运行自动生成 .env 配置文件
```

打开 http://127.0.0.1:8000 即是完整管理面板（前端已嵌入二进制）。

### 方式二：源码运行

```bash
# 1. 启动 ComfyUI（默认 http://127.0.0.1:8188）

# 2. 构建前端并嵌入
cd image_webui && npm ci && npm run build        # 产出 out/
rm -rf ../image_core/assets/* && cp -R out/. ../image_core/assets/

# 3. 构建并启动后端（默认 0.0.0.0:8000）
cd ../image_core && cargo run --release
```

### 开发模式（前后端分离热更新）

```bash
cd image_core && cargo run                       # 后端 :8000
cd image_webui && npx next dev -p 3200           # 前端 :3200（避开 Windows 保留端口段 2930-3129）
```

## 配置（.env）

首次启动自动在可执行文件旁生成带注释的 `.env`，支持全部参数：

| 变量 | 默认 | 说明 |
|---|---|---|
| `LISTEN_ADDR` | `0.0.0.0:8000` | 监听地址 |
| `COMFYUI_URL` | `http://127.0.0.1:8188` | ComfyUI 地址 |
| `REQUEST_TIMEOUT_S` | `300` | 单次生成超时（秒，含排队） |
| `MAX_CONCURRENT` | `2` | 并发执行上限，超额排队 |
| `IMAGE_RETENTION_HOURS` | `72` | 图片保留时长（0=永久） |
| `IMAGE_MAX_TOTAL_MB` | `1024` | 图片总容量上限（0=不限） |
| `DATA_DIR` | `./data` | 数据目录（SQLite + 图片） |
| `ADMIN_PASSWORD` | 空 | 管理面板密码，非空即启用鉴权 |

以上参数同时可在面板「设置」页在线修改（`LISTEN_ADDR`/`DATA_DIR` 需重启生效，页面会提示）。

## 对外 API（`Authorization: Bearer <sk-...>`）

| 格式 | 端点 |
|---|---|
| OpenAI DALL·E | `POST /v1/images/generations`、`POST /v1/images/edits`（图生图，multipart）、`POST /v1/images/variations`、`GET /v1/models` |
| 火山引擎 Seedream | `POST /volcengine/api/v3/images/generations` |
| SiliconFlow | `POST /siliconflow/v1/images/generations` |
| 智谱 CogView | `POST /zhipu/paas/v4/images/generations` |

```bash
curl -X POST http://127.0.0.1:8000/v1/images/generations \
  -H "Authorization: Bearer sk-xxx" \
  -H "Content-Type: application/json" \
  -d '{"model":"<模型ID或名称>","prompt":"a cute cat","size":"1024x1024"}'
```

## 面板功能

- **仪表盘**：请求统计、成功率、平均延迟、24h 请求量/延迟趋势图、模型用量 Top5、队列状态、图片存储；**仪表墙全屏模式**（15s 自动刷新）
- **统计**：1h/6h/24h/7d 趋势图，按密钥/按模型用量排行
- **模型管理**：导入 ComfyUI 工作流（粘贴/上传 JSON，自动识别 UI/API 两种格式；或从目录批量导入），自动探测参数映射，可手动编辑；按 kind 分类
- **API 密钥**：创建/启停/删除，每个密钥独立用量图表
- **请求日志**：筛选、分页、详情（含生成图片）
- **在线测试**：选模型直接出图；图生图模型支持上传参考图
- **设置**：全部运行参数在线修改、存储管理、连接检查
- **其他**：亮/暗主题切换、移动端响应式布局、管理密码登录

## 进阶特性

- **并发队列**：`MAX_CONCURRENT` 限制同时执行的 ComfyUI 任务，超额排队，`GET /admin/api/queue` 可观测
- **图片自动清理**：按保留时长与总容量双策略，后台每 10 分钟执行，也可手动触发
- **UI 格式工作流**：直接导入 ComfyUI 前端保存的 `nodes/links` 格式 JSON，自动转换为 API 格式（含 Reroute 解析、合并节点平铺字段回退探测）
- **统计聚合**：按小时/天桶聚合的时间序列，支持按密钥/模型过滤

## 构建发布（GitHub Actions）

推送 `v*` 标签自动触发六平台单文件构建并创建 Release：

Windows x86_64、Linux x86_64（含 22.04 兼容版）、Linux aarch64、macOS aarch64、macOS x86_64。

## 数据与目录

数据存于 `image_core/data/`（或 `DATA_DIR`）：SQLite 数据库 + 生成的图片。示例工作流在 `image_core/examples/`。
