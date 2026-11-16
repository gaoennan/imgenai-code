// imgenai 账户的常驻后台：登录、订阅额度、重置卡、下单与查单，全部转发给 imgenai 网站现有的接口。
//
// - 只保存网站返回的登录令牌，不保存密码；令牌写在拓展数据目录的 session.json（权限 0600）。
// - 每个方法都返回 { ok: true, ... } 或 { ok: false, code, message }，页面按 code 决定下一步，不靠抛错。
// - 只用 node:http / node:https，不依赖全局 fetch，Codex App 自带的 Node 版本旧一些也能跑。
import fs from "node:fs";
import path from "node:path";
import http from "node:http";
import https from "node:https";
import { spawn } from "node:child_process";

const DEFAULT_SITE = "https://imgenai.net";
/** 客户端专用的中转密钥名：登录后找这个名字的密钥，没有就新建一把。 */
const KEY_NAME = "桌面客户端";
const TIMEOUT_MS = 15000;
const MAX_JSON_BYTES = 2 * 1024 * 1024;
const MAX_IMAGE_BYTES = 2 * 1024 * 1024;

class ApiError extends Error {
  constructor(message, { status = 0, code = "" } = {}) {
    super(message);
    this.status = status;
    this.code = code;
  }
}

function normalizeSite(value) {
  const text = String(value || "").trim().replace(/\/+$/, "");
  return /^https?:\/\/[^\s/]+/i.test(text) ? text : DEFAULT_SITE;
}

/** 发一个 HTTP 请求，返回 { status, headers, body: Buffer }；超时、超长都按错误处理。 */
function rawRequest(method, url, { headers = {}, body, timeoutMs = TIMEOUT_MS, maxBytes = MAX_JSON_BYTES } = {}) {
  return new Promise((resolve, reject) => {
    let target;
    try {
      target = new URL(url);
    } catch {
      reject(new ApiError(`地址不合法：${url}`));
      return;
    }
    const client = target.protocol === "http:" ? http : https;
    const payload = body === undefined ? undefined : Buffer.from(body);
    const request = client.request(
      target,
      {
        method,
        headers: {
          "User-Agent": "imgenai-desktop",
          ...headers,
          ...(payload ? { "Content-Length": String(payload.length) } : {}),
        },
      },
      (response) => {
        const chunks = [];
        let size = 0;
        response.on("data", (chunk) => {
          size += chunk.length;
          if (size > maxBytes) {
            request.destroy(new ApiError("imgenai 返回的内容过大"));
            return;
          }
          chunks.push(chunk);
        });
        response.on("end", () => resolve({ status: response.statusCode || 0, headers: response.headers, body: Buffer.concat(chunks) }));
        response.on("error", reject);
      },
    );
    request.setTimeout(timeoutMs, () => request.destroy(new ApiError("连接 imgenai 超时，请检查网络后重试", { code: "TIMEOUT" })));
    request.on("error", (error) => {
      reject(error instanceof ApiError ? error : new ApiError(`连接 imgenai 失败：${error?.message || error}`, { code: "NETWORK" }));
    });
    if (payload) request.write(payload);
    request.end();
  });
}

/** 网站接口有的把结果放在 data 里，有的直接平铺；两种都认。 */
const unwrap = (json) => (json && typeof json === "object" && json.data !== undefined ? json.data : json);

/** 只留界面要用的用户字段，别把网站返回的其它信息落盘。 */
function pickUser(user, phone) {
  const source = user && typeof user === "object" ? user : {};
  return {
    id: source.id ?? null,
    phone: String(source.phone || phone || ""),
    nickname: String(source.nickname || source.username || source.name || ""),
  };
}

function normalizeKey(key) {
  const text = String(key || "").trim();
  if (!text) return "";
  return text.startsWith("sk-") ? text : `sk-${text}`;
}

/** 列表里能直接用的密钥：名字对得上，且不是打码后的（含 * 的说明网站只给了掩码）。 */
function pickToken(list) {
  return (Array.isArray(list) ? list : []).find((token) => token && token.name === KEY_NAME && typeof token.key === "string" && token.key && !token.key.includes("*")) || null;
}

function openExternal(url) {
  const command = process.platform === "darwin" ? "open" : process.platform === "win32" ? "cmd" : "xdg-open";
  const args = process.platform === "win32" ? ["/c", "start", "", url] : [url];
  const child = spawn(command, args, { detached: true, stdio: "ignore", windowsHide: true });
  child.on("error", () => {});
  child.unref();
}

export default ({ config, dataDir, log }) => {
  const site = normalizeSite(config?.siteBase);
  const sessionFile = path.join(dataDir || process.cwd(), "session.json");

  const readSession = () => {
    try {
      const session = JSON.parse(fs.readFileSync(sessionFile, "utf8"));
      return session && typeof session.token === "string" && session.token ? session : null;
    } catch {
      return null;
    }
  };
  const writeSession = (session) => {
    fs.mkdirSync(path.dirname(sessionFile), { recursive: true });
    fs.writeFileSync(sessionFile, JSON.stringify(session), { mode: 0o600 });
    try {
      fs.chmodSync(sessionFile, 0o600);
    } catch {
      // Windows 上 chmod 无意义，忽略。
    }
  };
  const clearSession = () => {
    try {
      fs.unlinkSync(sessionFile);
    } catch {
      // 本来就没有。
    }
  };

  /** 调网站接口：带上登录令牌，401 当作登录过期并清掉本地令牌；success:false 转成 ApiError。 */
  async function api(method, pathname, { body, auth = true } = {}) {
    const headers = { Accept: "application/json" };
    if (body !== undefined) headers["Content-Type"] = "application/json";
    if (auth) {
      const session = readSession();
      if (!session) throw new ApiError("请先登录 imgenai 账号", { status: 401, code: "NOT_LOGGED_IN" });
      headers.Authorization = `Bearer ${session.token}`;
    }
    const response = await rawRequest(method, site + pathname, { headers, body: body === undefined ? undefined : JSON.stringify(body) });
    let json = null;
    try {
      json = response.body.length ? JSON.parse(response.body.toString("utf8")) : null;
    } catch {
      json = null;
    }
    if (auth && response.status === 401) {
      clearSession();
      throw new ApiError("登录已过期，请重新登录", { status: 401, code: "SESSION_EXPIRED" });
    }
    if (response.status < 200 || response.status >= 300 || (json && json.success === false)) {
      throw new ApiError(json?.message || `imgenai 返回 HTTP ${response.status}`, { status: response.status, code: json?.code || "" });
    }
    if (!json || typeof json !== "object") throw new ApiError("imgenai 返回了无法识别的内容");
    return json;
  }

  /** 支付二维码是网站给的图片地址；管理器面板只能显示 data URL，这里下载后转成 data URL。 */
  async function imageDataUrl(url) {
    if (!/^https?:\/\//i.test(String(url || ""))) return "";
    const response = await rawRequest("GET", url, { headers: { Accept: "image/*" }, maxBytes: MAX_IMAGE_BYTES });
    const type = String(response.headers["content-type"] || "").split(";")[0].trim();
    if (response.status !== 200 || !type.startsWith("image/")) return "";
    return `data:${type};base64,${response.body.toString("base64")}`;
  }

  async function listTokens() {
    const data = unwrap(await api("GET", "/api/relay/tokens"));
    return Array.isArray(data) ? data : Array.isArray(data?.items) ? data.items : [];
  }

  /** 包一层：方法里抛的 ApiError 变成 { ok: false, code, message }，其它异常记日志后同样返回。 */
  const safe = (fn) => async (params) => {
    try {
      return { ok: true, ...(await fn(params || {})) };
    } catch (error) {
      if (!(error instanceof ApiError)) log?.(`imgenai-account: ${error?.stack || error}`);
      return { ok: false, code: error?.code || "", status: error?.status || 0, message: error?.message || String(error) };
    }
  };

  return {
    state: safe(async () => {
      const session = readSession();
      return { loggedIn: !!session, user: session?.user || null, site };
    }),

    login: safe(async ({ phone, password }) => {
      const account = String(phone || "").trim();
      const secret = String(password || "");
      if (!account || !secret) throw new ApiError("请填写手机号和密码", { code: "INVALID_INPUT" });
      const json = await api("POST", "/api/auth/login", { body: { phone: account, password: secret }, auth: false });
      if (!json.token) throw new ApiError(json.message || "登录失败", { code: "LOGIN_FAILED" });
      const user = pickUser(json.user, account);
      writeSession({ token: json.token, user, savedAt: Date.now() });
      return { user };
    }),

    logout: safe(async () => {
      clearSession();
      return {};
    }),

    overview: safe(async () => {
      const session = readSession();
      if (!session) return { loggedIn: false };
      const sub = unwrap(await api("GET", "/api/relay/subscription"));
      return { loggedIn: true, user: session.user, sub: sub && typeof sub === "object" ? sub : null };
    }),

    /** 取客户端专用密钥，没有就新建；没订阅时网站会拒绝新建（RELAY_NO_SUBSCRIPTION / RELAY_NOT_ENABLED）。 */
    ensureKey: safe(async () => {
      let token = pickToken(await listTokens());
      if (!token) {
        await api("POST", "/api/relay/tokens", { body: { name: KEY_NAME } });
        token = pickToken(await listTokens());
      }
      if (!token) throw new ApiError("没有拿到中转密钥，请到官网「API 中转」页面查看", { code: "KEY_UNAVAILABLE" });
      return { key: normalizeKey(token.key), name: token.name };
    }),

    freeTrial: safe(async () => {
      const json = await api("POST", "/api/relay/free-trial", { body: {} });
      return { message: json.message || "已开通免费试用" };
    }),

    useResetCard: safe(async () => {
      const json = await api("POST", "/api/relay/reset-card/use", { body: {} });
      return { message: json.message || "已使用重置卡" };
    }),

    /** kind = subscription（plan 为 paid / pro）或 pack（加油包）。金额由网站后端按档位计算，这里不传价格。 */
    createOrder: safe(async ({ kind, plan }) => {
      const body = kind === "pack" ? { type: "relay_pack" } : { type: "subscription", plan: plan === "pro" ? "pro" : "paid" };
      const order = unwrap(await api("POST", "/api/payment/create", { body }));
      if (!order?.tradeOrderId) throw new ApiError("创建订单失败，请稍后重试", { code: "ORDER_FAILED" });
      let qrDataUrl = "";
      try {
        qrDataUrl = await imageDataUrl(order.qrcodeUrl);
      } catch {
        qrDataUrl = "";
      }
      return { tradeOrderId: String(order.tradeOrderId), paymentUrl: String(order.paymentUrl || order.qrcodeUrl || ""), qrDataUrl };
    }),

    orderStatus: safe(async ({ tradeOrderId }) => {
      const id = String(tradeOrderId || "");
      if (!/^[A-Za-z0-9_-]{1,80}$/.test(id)) throw new ApiError("订单号不合法", { code: "INVALID_INPUT" });
      const json = await api("GET", `/api/payment/status/${encodeURIComponent(id)}`);
      return { status: String(json?.data?.status || json?.status || "pending") };
    }),

    /** 在系统浏览器里打开：只接受 http(s) 地址。 */
    openUrl: safe(async ({ url }) => {
      const text = String(url || "");
      if (!/^https?:\/\//i.test(text)) throw new ApiError("只能打开网页地址", { code: "INVALID_INPUT" });
      openExternal(text);
      return {};
    }),

    /** 打开官网的某个页面（注册、找回密码、财务中心等都在官网完成）。 */
    openSite: safe(async ({ hash }) => {
      const suffix = /^[A-Za-z0-9_-]{0,40}$/.test(String(hash || "")) && hash ? `/#${hash}` : "/";
      openExternal(site + suffix);
      return {};
    }),
  };
};
