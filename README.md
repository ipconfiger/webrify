# Webrify Turnstile

自托管、单租户的人机验证组件——一个 Cloudflare Turnstile 风格的开源替代方案。
用 **Rust 核心（编译为 WASM 前端 + Axum 后端）+ React/TypeScript widget** 实现，
通过四层验证（PoW 挑战-响应、环境指纹、行为分析、风险评分）+ 自适应难度抵御自动化攻击。

部署形态：**单个二进制**（widget 内嵌）+ **Redis**。无数据库、无前端单独部署。

---

## 目录

- [功能特性](#功能特性)
- [架构](#架构)
- [快速开始](#快速开始)
- [配置](#配置)
- [在项目中集成](#在项目中集成)
- [验证流程](#验证流程)
- [API 端点](#api-端点)
- [安全与合规](#安全与合规)
- [CLI：管理 allowed\_origins](#cli管理-allowed_origins)
- [可观测性](#可观测性)
- [项目结构](#项目结构)
- [测试](#测试)

---

## 功能特性

### 四层验证（核心）

| 层         | 做什么                                                     | 实现要点                                                                                          |
| ---------- | ---------------------------------------------------------- | ------------------------------------------------------------------------------------------------- |
| **PoW**        | 证明客户端做了真实计算                                     | Hashcash 风格 SHA-256，难度=前导零比特数；客户端在 **Web Worker + WASM** 里求解，不阻塞主线程       |
| **环境指纹**   | 识别无头浏览器 / 模拟器                                     | Canvas / WebGL / AudioContext / 字体 / navigator 等信号 → 128-bit 哈希；**绑入 PoW seed**，解不可跨客户端复用 |
| **行为分析**   | 识别脚本式交互                                             | 鼠标移动 / 点击 / 按键节奏的变异系数（CV）→ 0-100 人类相似度评分；WASM 计算                         |
| **风险评分**   | 综合决策                                                   | 上述信号 + solve-time → 0-100 风险分 → `Allow` / `Escalate` / `Deny`；`Escalate/Deny` 反哺难度        |

### 安全

- **HMAC 签名挑战**：服务端对全部绑定参数（`algorithm|salt|challenge|difficulty|maxnumber|expires_at|origin`）签名，客户端无法篡改难度/替换 origin；比对用 constant-time
- **原子防重放**：每个挑战单次使用，Redis `SET … NX EX`（TOCTOU-safe）
- **fail-closed**：Redis 宕机时拒绝验证（503），绝不静默放行；`/ready` 反映 Redis 健康
- **每 IP 限流**：默认 10 req/s/IP（固定窗口，仅生产生效）
- **自适应难度**：被标记为高风险的对端下次拿到更难的挑战（每加 1 bit ≈ 翻倍计算量），封顶 24 bit，不会锁死真实用户
- **GDPR**：原始信号在浏览器本地哈希，永不传输/存储；提供 `disableFingerprint` 的 PoW-only 降级路径
- `#![forbid(unsafe_code)]` 覆盖全部安全关键 Rust crate

### 运维

- **单二进制部署**：widget（JS+WASM）经 `rust-embed` 编译期内嵌进服务端二进制
- **结构化 JSON 日志**（`tracing`）
- **`/metrics` Prometheus 端点**（挑战签发数、验证成功/失败计数）
- **`webrify sitekey` CLI** 管理 `allowed_origins`
- **优雅关闭**（SIGTERM / Ctrl-C）

---

## 架构

```
webrify/
├── crates/
│   ├── turnstile-core/      # 平台无关核心（同一份逻辑编译为 WASM + native）
│   │   └── src/{pow, protocol, fingerprint, behavior, risk, rng, lib}.rs
│   ├── turnstile-wasm/      # wasm32 绑定（#[wasm_bindgen]），widget 的 Web Worker 加载它
│   │   └── src/lib.rs
│   └── turnstile-server/    # Axum 原生服务（二进制名 `webrify`）
│       └── src/{main, config, error, state, store, hmac, jwt, rate_limit, metrics, cli, extract, lib}
│           └── routes/{challenge, verify, health, metrics, widget, demo, mod}.rs
├── packages/turnstile-widget/   # React + Vite + TypeScript（构建产物 dist/ 被 server 嵌入）
│   └── src/{TurnstileWidget, pow-worker, fingerprint, behavior, types, index}.{tsx,ts}
├── docs/{proposal, operations, research-report, poW-captcha-research-report}.md
├── Cargo.toml                  # [workspace]，default-members = turnstile-server
├── rust-toolchain.toml
└── justfile                    # 构建编排（wasm-pack → vite → cargo）
```

**核心设计**：`turnstile-core` 同时编译到 `wasm32-unknown-unknown`（前端 WASM）和 native（后端），
PoW / 指纹 / 行为 / 风险逻辑单点定义，前后端共用——杜绝逻辑漂移。

**构建管线**（顺序）：
```
wasm-pack build  →  vite build（widget 消费 wasm pkg）  →  cargo build（server 经 rust-embed 内嵌 widget dist/）
```

---

## 快速开始

### 前置依赖

- **Rust**（stable；`rust-toolchain.toml` 已固定 + `wasm32-unknown-unknown` target）
- **wasm-pack**（把 `turnstile-core` 编译为 WASM）
- **Node + npm**（构建 widget；仅构建期需要，运行期不需要）
- **Redis**（唯一的运行期外部依赖）

### 构建

```bash
# 一条命令（推荐）：wasm → widget → server
just build-all

# 或分步
just build-wasm        # crates/turnstile-wasm → pkg/
just build-widget      # 复制 pkg 进 widget，然后 vite build → dist/
cargo build -p turnstile-server   # rust-embed 把 dist/ 烤进二进制
```

> `crates/turnstile-server/build.rs` 监视 `dist/` 并发 `rerun-if-changed`，
> 所以 widget 重建后下一次 `cargo build` 会自动重新内嵌，无需手动 `cargo clean`。

### 运行

```bash
# 1. 启动 Redis
redis-server &            # 或 brew services start redis / docker run -p 6379:6379 -d redis

# 2. 启动 webrify（必需配置走环境变量）
WEBRIFY_HMAC_KEY="$(openssl rand -hex 32)" \
WEBRIFY_JWT_KEY="$(openssl rand -hex 32)" \
WEBRIFY_ALLOWED_ORIGINS="https://your-app.example.com" \
./target/debug/webrify
# -> listening on 127.0.0.1:3000
```

浏览器访问 `http://127.0.0.1:3000/demo` 可看到同源 demo 页跑通整个验证流程。

---

## 配置

启动时一次性加载（Parse-Don't-Validate）。来源（后者覆盖前者）：可选 TOML 文件 → 环境变量（前缀 `WEBRIFY_`）。

| 字段                  | 环境变量                      | 默认值                        | 说明                                                              |
| --------------------- | ----------------------------- | ----------------------------- | ----------------------------------------------------------------- |
| `bind_addr`           | `WEBRIFY_BIND_ADDR`           | `127.0.0.1:3000`              | 监听地址                                                          |
| `redis_url`           | `WEBRIFY_REDIS_URL`           | `redis://127.0.0.1:6379/0`    | Redis 连接串（防重放 + 升级计数）                                  |
| `hmac_key`            | `WEBRIFY_HMAC_KEY`            | **（必填）**                  | 挑战 HMAC 签名密钥                                                |
| `jwt_key`             | `WEBRIFY_JWT_KEY`             | **（必填）**                  | 验证通过后签发的 JWT（HS256）密钥                                  |
| `allowed_origins`     | `WEBRIFY_ALLOWED_ORIGINS`     | **（必填）**                  | 允许使用本服务的 origin；env 用逗号分隔，TOML 里写数组。CORS + 挑战绑定 |
| `difficulty`          | `WEBRIFY_DIFFICULTY`          | `14`                          | 基础 PoW 难度（前导零比特数）。~14 ≈ 桌面端 1 秒。封顶 24           |
| `maxnumber`           | `WEBRIFY_MAXNUMBER`           | `100_000`                     | PoW nonce 搜索空间上限（客户端侧的求解边界）                       |
| `challenge_ttl_secs`  | `WEBRIFY_CHALLENGE_TTL_SECS`  | `300`                         | 挑战有效期（秒）                                                  |
| `jwt_ttl_secs`        | `WEBRIFY_JWT_TTL_SECS`        | `900`                         | 签发的 JWT 有效期（秒）                                            |
| `allow_js_disabled`   | `WEBRIFY_ALLOW_JS_DISABLED`   | `false`                       | 默认 fail-closed；`true` 开启无 PoW 的高风险路径（MVP 未启用）      |

### TOML 形式（`webrify.toml`）

```toml
bind_addr = "0.0.0.0:3000"
redis_url = "redis://redis:6379/0"
hmac_key = "..."            # 也可只走 env
jwt_key = "..."
difficulty = 12
allowed_origins = ["https://app.example.com", "https://www.example.com"]
```

> **密钥生成**：`openssl rand -hex 32`。密钥仅服务端持有，切勿入库/提交。
> **轮换**：改 env/TOML 后重启即可；JWT 头部加 `kid` 以支持平滑轮换是后续硬化项。

---

## 在项目中集成

Widget 由二进制在 `/widget/turnstile.js` 提供。在受保护页面上加一个容器并挂载：

```html
<div id="webrify-ts"></div>

<script type="module">
  import { mount } from "https://webrify.yourhost/widget/turnstile.js";

  mount(document.getElementById("webrify-ts"), {
    endpoint: "https://webrify.yourhost",   // 空字符串 = 同源
    onVerify: (token) => {
      // 把 token 发到【你自己的后端】，后端调 /siteverify 复核（见下方说明）
      fetch("/your-login", { method: "POST", body: JSON.stringify({ token }) });
    },
    onError: (msg) => console.warn("turnstile:", msg),
    // disableFingerprint: true,   // GDPR 无指纹 / PoW-only 降级
  });
</script>
```

> 挂载后，widget 在用户点击「验证」时**自动完成**下方的[验证流程](#验证流程)：拉取挑战 → Web Worker 里算指纹/行为/PoW → 提交验证 → 拿到 JWT 经 `onVerify(token)` 回调。宿主页**不需要**自己调 `/challenge` 或 `/verify`，只需把 `token` 转发给自己的后端。

### npm 包 `webrify-turnstile`：三种集成方式

#### 1. 默认组件（最简单）

```tsx
import { TurnstileWidget } from "webrify-turnstile";

<TurnstileWidget
  endpoint="https://webrify.yourhost"
  onVerify={(token) => sendToBackend(token)}
  onError={(msg) => console.warn(msg)}
  // disableFingerprint: true,       // GDPR 无指纹降级
  // className: "my-captcha-btn",    // 自定义 CSS 类
  // style: {{ fontSize: 16 }},      // 覆盖默认内联样式
/>
```

#### 2. 头部分离（完全自定义 UI）

```tsx
import { useTurnstile } from "webrify-turnstile";

function MyCaptcha() {
  const { status, errorMessage, verify, reset } = useTurnstile({
    endpoint: "https://webrify.yourhost",
    onVerify: (token) => sendToBackend(token),
  });

  return (
    <div>
      <button onClick={verify} disabled={status !== "idle" && status !== "error"}>
        {status === "idle" && "点击验证"}
        {status === "fetching" && "获取挑战…"}
        {status === "solving" && "计算中…"}
        {status === "verifying" && "验证中…"}
        {status === "success" && "✓ 已通过"}
        {status === "error" && `✗ ${errorMessage}（点击重试）`}
      </button>
      {status === "error" && <button onClick={reset}>重置</button>}
    </div>
  );
}
```

> `useTurnstile` 返回值：`status`（6 种状态）、`errorMessage`（错误信息）、`verify()`（触发验证）、`reset()`（重置到 idle）。

#### 3. 命令式挂载（非 React 页面）

```html
<div id="webrify-ts"></div>
<script type="module">
  import { mount } from "webrify-turnstile";

  mount(document.getElementById("webrify-ts"), {
    endpoint: "https://webrify.yourhost",
    onVerify: (token) => sendToBackend(token),
  });
</script>
```

### 受保护页面的 CSP 要求

Widget 会加载脚本 + 启动 Worker + 实例化 WASM，严格的 CSP 需放行：

```
script-src https://webrify.yourhost;
worker-src https://webrify.yourhost;     // PoW worker chunk
connect-src https://webrify.yourhost;    // /challenge + /verify 的 fetch
wasm-unsafe-eval;                         // WASM 实例化（PoW 求解器）
```

> `endpoint` 指向的 webrify 主机与受保护页面通常是**跨源**的——webrify 已对 `allowed_origins` 配置了 CORS 反射 + 预检。

---

## 验证流程

widget 挂载后，用户点击「验证」触发以下流程。**左侧是 widget（浏览器）自动做的，右侧是 webrify server 的 8 步校验**；宿主页全程只接触首尾两端（`mount()` 与 `onVerify(token)`）。

```
浏览器 widget（受保护页）              webrify server                      Redis
     │                                    │                               │
     │ ① POST /challenge                   │                               │
     │    Header: Origin: https://yourapp │                               │
     │ ─────────────────────────────────▶ │ Origin ∈ allowed_origins ?     │
     │                                    │ 生成 challenge(128bit)+salt     │
     │                                    │ difficulty = adjust(base, 该IP升级数)
     │                                    │ HMAC 签名全部绑定参数            │
     │ ◀──────────── Challenge (JSON) ────│ （无状态签名，不入库）           │
     │                                                                      │
     │ ② Web Worker（WASM，离主线程）：                                     │
     │    指纹 = fingerprint_hash(Canvas+WebGL+Audio+navigator 信号)         │
     │    行为 = behavior_score(鼠标/按键/点击 的 CV)   // 0-100            │
     │    seed   = hex_decode(challenge) ‖ hex_decode(fingerprint)           │
     │    nonce  = solve_bounded(seed, difficulty, maxnumber)                │
     │                                                                      │
     │ ③ POST /verify  (VerifyRequest JSON，见下方示例)                      │
     │ ─────────────────────────────────▶ │ ❶ 验 HMAC（constant-time）      │
     │                                    │ ❷ expires_at 未过期             │
     │                                    │ ❸ 原子标记已花 ──────────────▶ │ SET NX EX
     │                                    │ ❹ 重算 SHA-256(seed‖nonce) 验前导零 ≥ difficulty
     │                                    │ ❺ nonce ≤ maxnumber            │
     │                                    │ ❻ origin ∈ allowed_origins     │
     │                                    │ ❼ 风险评分 → Allow/Escalate/Deny │
     │                                    │    Escalate/Deny → INCR ─────▶ │ (下次提难度)
     │                                    │ ❽ 签发 JWT（HS256，绑 origin）   │
     │ ◀──────── { success, token, expires_at } ────────                     │
     │                                                                      │
     │ ④ onVerify(token) → 宿主页把 token 发到自己的后端做后续业务             │
```

### 请求 / 响应示例

**`POST /challenge`** → `200`（服务端签发，原样返回给客户端）
```json
{
  "algorithm": "SHA-256",
  "salt": "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4",
  "challenge": "c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6",
  "difficulty": 14,
  "maxnumber": 100000,
  "expires_at": 1758000000,
  "origin": "https://yourapp.example.com",
  "signature": "e5f6a1b2c3d4e5f6...(HMAC-SHA256 hex)"
}
```

**`POST /verify`** ← 请求体（`VerifyRequest`，客户端把挑战字段原样回传 + 附上求解结果）
```json
{
  "algorithm": "SHA-256",
  "challenge": "c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6",
  "salt": "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4",
  "difficulty": 14,
  "maxnumber": 100000,
  "expires_at": 1758000000,
  "origin": "https://yourapp.example.com",
  "signature": "e5f6a1b2c3d4e5f6...(回传，服务端重算比对)",
  "nonce": 4721,
  "idempotency_key": "550e8400-e29b-41d4-a716-446655440000",
  "fingerprint": "9a8b7c6d...(128-bit 指纹 hex；disableFingerprint 时省略)",
  "behavior_score": 82
}
```

**`POST /verify`** → `200`
```json
{ "success": true, "token": "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9...(JWT)", "expires_at": 1758000900 }
```

> **PoW seed 构造**：`seed = hex_decode(challenge) ‖ hex_decode(fingerprint)`。`fingerprint` 缺省（PoW-only 降级）时 seed 只含 challenge 字节——服务端按相同规则重算，两端必须一致。
> **`onVerify(token)`** 收到的 `token` 即上方响应里的 `token` 字段（JWT），不是整个响应对象。

### 依赖方后端如何验证 token

宿主页前端拿到 token 后发给**自己的后端**，后端调 webrify 的 `/siteverify` 复核（无需持有签名密钥）：

**`POST /siteverify`** ← 请求体
```json
{ "token": "eyJ0eXAi...(widget 拿到的 JWT)", "origin": "https://yourapp.example.com" }
```
> `origin` 建议传——webrify 会校验 token 的 `aud` 与之一致，防止跨站复用。

**`POST /siteverify`** → `200`
```json
{ "success": true, "origin": "https://yourapp.example.com", "expires_at": 1758000900, "error": null }
```
验证失败时 `success: false` 且 `error` 给出原因（签名错 / 过期 / origin 不匹配）。

**后端调用示例**：
```bash
curl -X POST https://webrify.yourhost/siteverify \
  -H 'content-type: application/json' \
  -d '{"token":"eyJ0eXAi...","origin":"https://yourapp.example.com"}'
```
```js
// Node
const r = await fetch("https://webrify.yourhost/siteverify", {
  method: "POST",
  headers: { "content-type": "application/json" },
  body: JSON.stringify({ token, origin: "https://yourapp.example.com" }),
});
const { success } = await r.json();
if (!success) throw new Error("人机验证失败");
```

> 也可不走 `/siteverify`，由依赖方后端用共享的 `jwt_key` 自行 HS256 验签（单租户自托管、同一运维方时可行）。但 `/siteverify` 更安全——密钥不离开 webrify。

---

## API 端点

| 方法 | 路径          | 用途                                                          |
| ---- | ------------- | ------------------------------------------------------------- |
| POST | `/challenge`  | 签发 HMAC 签名挑战（要求 `Origin` 在 allowlist 内）           |
| POST | `/verify`     | 提交 nonce + 指纹 + 行为评分 → 返回 JWT（或错误）             |
| POST | `/siteverify` | 依赖方后端复核 token 有效性（无需共享密钥）                    |
| GET  | `/health`     | 存活探针（进程在跑）                                          |
| GET  | `/ready`      | 就绪探针（Redis 可达）；宕机时 503（fail-closed）             |
| GET  | `/metrics`    | Prometheus 文本格式（计数器）                                 |
| GET  | `/widget/*`   | 内嵌的 widget 资产（JS/worker/wasm）                         |
| GET  | `/demo`       | 同源 demo 页，演示 widget 完整流程                            |

---

## 安全与合规

- **fail-closed**：Redis 宕机 → 503 拒绝验证（绝不绕过防重放）；负载均衡器可据 `/ready` 摘除不健康实例
- **防重放**：每个挑战单次使用（Redis 原子 `SET … NX EX`）
- **防篡改**：服务端 HMAC 签名全部绑定参数；客户端无法放宽难度或换 origin；constant-time 比对
- **PoW 绑指纹**：解不可跨客户端复用——每个环境都得自己算
- **限流**：每 IP 固定窗口（默认 10 req/s/IP），仅生产生效
- **GDPR 数据极小化**：原始信号在浏览器本地哈希，只传 128-bit 哈希；可 `disableFingerprint` 走纯 PoW；不写 cookie
- **审计姿态**：设计上全短暂态，无持久化用户尝试记录；如需可查询历史，把 JSON 日志外发到 Loki/ELK（或后续加 Postgres）
- `#![forbid(unsafe_code)]` 覆盖 `turnstile-core` / `turnstile-wasm` / `turnstile-server`

---

## CLI：管理 allowed_origins

```bash
webrify sitekey list                          # 列出当前 allowed_origins
webrify sitekey add https://app2.example.com  # 追加
webrify sitekey remove https://old.example.com
webrify sitekey list --config /etc/webrify.toml
```

以通用 `toml::Value` 读改写 TOML 文件（保留其它字段）；改动**重启后生效**。

---

## 可观测性

- **日志**：结构化 JSON（`RUST_LOG=info`），适合机器解析/外发
- **`/metrics`**（Prometheus 文本）：
  - `webrify_challenges_issued_total`（counter）
  - `webrify_verifies_total{result="success|failed"}`（counter）

---

## 项目结构

见 [架构](#架构) 一节的目录树。关键设计：

- **`turnstile-core`** 双目标：同一份 Rust 逻辑编译为浏览器 WASM（前端求解）+ native（后端验证），逻辑不漂移
- **`turnstile-wasm`** 薄 `wasm_bindgen` 层：`solve_challenge`（接指纹）、`fingerprint_hash`、`behavior_score`
- **`turnstile-server`** Axum：路由 / Redis / HMAC / JWT / 限流 / 指标 / CLI
- **`turnstile-widget`** React + Vite 库模式：组件 + Web Worker（WASM 内联、主线程不阻塞）+ `mount()` 供非 React 页

---

## 测试

- **74 个 Rust 测试** green（`cargo test --workspace`），覆盖：PoW 求解/验证边界、指纹确定性、行为评分、风险阈值、HMAC 篡改检测、Redis 防重放/升级计数、/metrics 计数
- **wasm32 双目标**编译验证（`cargo check --target wasm32-unknown-unknown`）
- **Playwright 浏览器 E2E**：`/demo` → 点击 → `/challenge` → WASM worker（指纹+行为+PoW）→ `/verify` → 签发 JWT，端到端跑通
- `cargo clippy --all-targets -- -D warnings` + `cargo fmt --check` 全 clean

```bash
cargo test --workspace                 # 全部 Rust 单元 + 集成测试
cargo check -p turnstile-core --target wasm32-unknown-unknown   # 双目标
```

---

## 后续路线（已实现 Phases 1-4，提案目标达成）

- **已交付**：四层验证 + 自适应难度 + 限流 + GDPR 降级 + a11y + 指标 + CLI + 运维文档
- **可继续**：Ed25519 JWT 升级（非对称，依赖方可公钥验签）、分布式 Redis、Grafana 仪表盘、sitekey 热加载

详细运维/集成参考见 [`docs/operations.md`](docs/operations.md)；设计渊源见 [`docs/proposal.md`](docs/proposal.md)。
