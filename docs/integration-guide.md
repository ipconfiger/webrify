# Webrify Turnstile 集成指南

> **面向 AI Agent**：读完本文档即可完成前端集成和后端验证。标注 `[需用户提供]` 的配置项需向用户提问补全。

---

## 1. 概述

Webrify Turnstile 是一个自托管的、类 Cloudflare Turnstile 的人机验证组件。用户点击"Verify you are human"按钮后，浏览器在后台执行 PoW（工作量证明）、指纹采集和行为分析，通过后签发 JWT token。你的后端用 `/siteverify` 接口验证 token 真伪。

**组件特点：**
- 纯前端运行，无需加载第三方脚本
- 浏览器端执行 SHA-256 PoW，不依赖 reCAPTCHA/hCaptcha
- 内置行为分析（鼠标轨迹、点击节奏、按键节奏）识别自动化操作
- JWT token 绑定 origin，防止跨站复用
- 支持 GDPR 友好模式（`disableFingerprint: true` 关闭指纹采集）

---

## 2. 服务端部署

> **[需用户提供]** 把 Webrify 服务部署到哪台机器？IP 或域名是什么？端口用多少？

```bash
# 克隆仓库
git clone https://github.com/your-org/webrify.git && cd webrify

# Docker 部署（推荐）
docker compose up -d

# 或手动构建运行
./scripts/docker-build.sh && docker compose up -d
```

**必需环境变量**（在 `docker-compose.yml` 中配置）：

| 变量 | 说明 | 示例 |
|---|---|---|
| `WEBRIFY_HMAC_KEY` | HMAC 签名密钥（32+ 字符随机串） | `openssl rand -hex 32` 生成 |
| `WEBRIFY_JWT_KEY` | JWT 签发密钥（32+ 字符随机串） | `openssl rand -hex 32` 生成 |
| `WEBRIFY_ALLOWED_ORIGINS` | 允许的来源域名，逗号分隔；`*` 表示全部 | `https://your-site.com` 或 `*` |

**服务端点一览：**

| 方法 | 路径 | 用途 |
|---|---|---|
| `POST` | `/challenge` | 签发 PoW 挑战 |
| `POST` | `/verify` | 提交 PoW 解 → 获取 JWT |
| `POST` | `/siteverify` | 后端验证 token（**你的后端调用这个**） |
| `GET` | `/widget/turnstile.js` | 前端组件脚本 |
| `GET` | `/health` | 存活检查 |
| `GET` | `/ready` | 就绪检查（含 Redis） |

---

## 3. 前端集成

> **[需用户提供]** 你的前端是什么技术栈？（React / Vue / 原生 HTML）服务端地址是什么？

### 3.1 React

```tsx
import { TurnstileWidget } from "https://your-webrify-server.com/widget/turnstile.js";

// 或通过 local install:
// import { TurnstileWidget } from "@webrify/turnstile-widget";

function LoginPage() {
  const handleVerify = (token: string) => {
    // 把 token 发给你的后端验证
    fetch("/api/login", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ turnstile_token: token, /* 其他登录字段 */ }),
    });
  };

  return (
    <TurnstileWidget
      endpoint="https://your-webrify-server.com"  // [需用户提供]
      onVerify={handleVerify}
      onError={(msg) => console.error("人机验证失败:", msg)}
      // disableFingerprint={true}  // GDPR 友好模式
    />
  );
}
```

### 3.2 原生 HTML / Vue / 其他框架

```html
<!-- 加载组件 -->
<script type="module">
  import { mount } from "https://your-webrify-server.com/widget/turnstile.js";
  //                                          ^^^^^^^^^^^^^^^^ [需用户提供]

  mount(document.getElementById("webrify-ts"), {
    endpoint: "https://your-webrify-server.com",  // [需用户提供]
    onVerify: (token) => {
      // 把 token 发给你的后端验证
      fetch("/api/login", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ turnstile_token: token }),
      });
    },
    onError: (msg) => {
      alert("验证失败: " + msg);
    },
  });
</script>

<div id="webrify-ts"></div>
```

### 3.3 TurnstileOptions 完整参考

```typescript
interface TurnstileOptions {
  /** Webrify 服务端地址。空字符串 = 同源（前后端同域名）。默认 "" */
  endpoint?: string;

  /** 验证成功后回调，参数为 JWT token 字符串。**必填** */
  onVerify: (token: string) => void;

  /** 验证失败时回调，参数为错误信息。可选 */
  onError?: (message: string) => void;

  /** 关闭浏览器指纹采集（GDPR PoW-only 模式）。默认 false */
  disableFingerprint?: boolean;
}
```

---

## 4. 后端验证 Token

> **[需用户提供]** 你的后端用什么语言 / 框架？

前端拿到 JWT token 后，请求你的后端（如 `/api/login`），你的后端再调用 Webrify 的 `/siteverify` 验证 token 真伪。

### 4.1 接口说明

**请求：**
```
POST https://your-webrify-server.com/siteverify
Content-Type: application/json

{
  "token": "eyJ0eXAiOiJKV1Q...",       // 前端传来的 JWT
  "origin": "https://your-site.com"    // 可选，建议传（绑定验证）
}
```

**响应（永远 HTTP 200，通过 `success` 字段判断）：**
```json
// 成功
{ "success": true, "origin": "https://your-site.com", "expires_at": 1758000900 }

// 失败
{ "success": false, "error": "token expired" }
```

**关键点：** 服务端永远返回 HTTP 200。**必须检查 `success` 字段**，不要根据 HTTP 状态码判断。

### 4.2 代码示例

#### Node.js / Express

```js
const WEBBRIFY_SERVER = "https://your-webrify-server.com"; // [需用户提供]

app.post("/api/login", async (req, res) => {
  const { turnstile_token, username, password } = req.body;

  // 1. 验证 Turnstile token
  const verifyRes = await fetch(`${WEBBRIFY_SERVER}/siteverify`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      token: turnstile_token,
      origin: "https://your-site.com",  // [需用户提供]
    }),
  });
  const result = await verifyRes.json();

  if (!result.success) {
    return res.status(403).json({ error: "人机验证失败: " + result.error });
  }

  // 2. 业务登录逻辑
  // ...
});
```

#### Python / Flask

```python
import requests

WEBBRIFY_SERVER = "https://your-webrify-server.com"  # [需用户提供]

@app.route("/api/login", methods=["POST"])
def login():
    data = request.get_json()
    token = data.get("turnstile_token")

    # 1. 验证 Turnstile token
    r = requests.post(f"{WEBBRIFY_SERVER}/siteverify", json={
        "token": token,
        "origin": "https://your-site.com",  # [需用户提供]
    })
    result = r.json()

    if not result.get("success"):
        return {"error": f"人机验证失败: {result.get('error')}"}, 403

    # 2. 业务登录逻辑
    # ...
```

#### Go

```go
const webrifyServer = "https://your-webrify-server.com" // [需用户提供]

func loginHandler(w http.ResponseWriter, r *http.Request) {
    var body struct {
        TurnstileToken string `json:"turnstile_token"`
    }
    json.NewDecoder(r.Body).Decode(&body)

    // 1. 验证 Turnstile token
    payload, _ := json.Marshal(map[string]string{
        "token":  body.TurnstileToken,
        "origin": "https://your-site.com", // [需用户提供]
    })
    resp, _ := http.Post(webrifyServer+"/siteverify", "application/json",
        strings.NewReader(string(payload)))
    var result struct {
        Success bool   `json:"success"`
        Error   string `json:"error"`
    }
    json.NewDecoder(resp.Body).Decode(&result)

    if !result.Success {
        http.Error(w, "人机验证失败: "+result.Error, 403)
        return
    }

    // 2. 业务登录逻辑
    // ...
}
```

---

## 5. 完整示例：一个带人机验证的登录页面

```html
<!DOCTYPE html>
<html lang="zh-CN">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>登录 - 人机验证</title>
  <style>
    body { font-family: system-ui; display: flex; justify-content: center;
           align-items: center; min-height: 100vh; margin: 0; background: #f5f5f5; }
    .card { background: #fff; padding: 2rem; border-radius: 8px;
            box-shadow: 0 2px 8px rgba(0,0,0,0.1); width: 320px; }
    input { width: 100%; padding: 8px 12px; margin: 8px 0; border: 1px solid #ddd;
            border-radius: 4px; box-sizing: border-box; }
    button[type="submit"] { width: 100%; padding: 10px; background: #1a73e8;
            color: #fff; border: none; border-radius: 4px; cursor: pointer; margin-top: 1rem; }
    button[type="submit"]:disabled { opacity: 0.5; cursor: not-allowed; }
  </style>
</head>
<body>
  <div class="card">
    <h2>登录</h2>
    <form id="login-form">
      <input type="text" id="username" placeholder="用户名" required>
      <input type="password" id="password" placeholder="密码" required>
      <div id="webrify-ts"></div>
      <button type="submit" id="submit-btn" disabled>登录</button>
    </form>
    <p id="status"></p>
  </div>

  <script type="module">
    import { mount } from "https://your-webrify-server.com/widget/turnstile.js";
    //                          ^^^^^^^^^^^^^^^^ [需用户提供]

    const WEBBRIFY_SERVER = "https://your-webrify-server.com"; // [需用户提供]
    let turnstileToken = null;

    // 挂载人机验证组件
    mount(document.getElementById("webrify-ts"), {
      endpoint: WEBBRIFY_SERVER,
      onVerify: (token) => {
        turnstileToken = token;
        document.getElementById("submit-btn").disabled = false;
        document.getElementById("status").textContent = "✓ 人机验证通过";
      },
      onError: (msg) => {
        turnstileToken = null;
        document.getElementById("submit-btn").disabled = true;
        document.getElementById("status").textContent = "⚠ " + msg;
      },
    });

    // 表单提交
    document.getElementById("login-form").addEventListener("submit", async (e) => {
      e.preventDefault();
      if (!turnstileToken) return alert("请先完成人机验证");

      const res = await fetch("/api/login", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          username: document.getElementById("username").value,
          password: document.getElementById("password").value,
          turnstile_token: turnstileToken,
        }),
      });
      const data = await res.json();
      document.getElementById("status").textContent =
        res.ok ? "✓ 登录成功" : "✗ " + (data.error || "登录失败");
    });
  </script>
</body>
</html>
```

---

## 6. 集成检查清单

部署前确认以下步骤：

- [ ] Webrify 服务已启动，`/health` 返回 `ok`
- [ ] 已配置 `WEBRIFY_HMAC_KEY` 和 `WEBRIFY_JWT_KEY`（各至少 32 字符随机串）
- [ ] 已配置 `WEBRIFY_ALLOWED_ORIGINS` 包含你的网站域名（或设为 `*`）
- [ ] 前端 `endpoint` 指向正确的 Webrify 服务器地址
- [ ] 后端 `/siteverify` 调用中检查了 `success` 字段（不是 HTTP 状态码）
- [ ] 如果前后端同源，`endpoint` 可留空；否则必须填入完整 URL

---

## 7. 常见问题

**Q: 按钮点击后一直转圈不结束？**
A: 打开浏览器开发者工具 → Network 面板，检查 `/challenge` 和 `/verify` 请求是否被 CORS 拦截。确认 `WEBRIFY_ALLOWED_ORIGINS` 包含当前页面的 origin。

**Q: 非 localhost 的 HTTP 页面报 `crypto.randomUUID is not a function`？**
A: 已在 v2 版本中通过 polyfill 修复。确保使用的是最新版 widget。

**Q: 如何在 Vue 中使用？**
A: 用 3.2 节的原生 JS 方式 `mount()`，在 `onMounted()` 生命周期中调用。

**Q: Token 的有效期多久？**
A: 默认 900 秒（15 分钟），可通过 `WEBRIFY_JWT_TTL_SECS` 配置。
