//! imgenai 分发版：「imgenai 账户」拓展的后台，内置在客户端里，不依赖 Node。
//!
//! 账户页经 `ctx.backend.call(方法, 参数)` 调到这里（[`crate::extension_backend::call`] 遇到内置的
//! imgenai-account 拓展时直接转过来，不起 Node 进程）。Windows 上多数人既没装 Node 也没装 Codex App，
//! 原来的 Node 后台在那里起不来，账户页连登录都做不了。
//!
//! - 只保存网站返回的登录令牌，不保存密码；令牌写在拓展数据目录的 `session.json`（权限 0600），
//!   位置与原来的 Node 后台相同，升级后不用重新登录。
//! - 每个方法都返回 `{ ok: true, ... }` 或 `{ ok: false, code, status, message }`，页面按 code 决定下一步。
//! - 网络请求和客户端其它外部请求一样走系统代理 / 代理环境变量。

use std::path::{Path, PathBuf};
use std::time::Duration;

use base64::Engine as _;
use serde_json::{Map, Value, json};

const DEFAULT_SITE: &str = "https://imgenai.net";
/// 客户端专用的中转密钥名：登录后找这个名字的密钥，没有就新建一把。
const KEY_NAME: &str = "桌面客户端";
const USER_AGENT: &str = "imgenai-desktop";
const TIMEOUT: Duration = Duration::from_secs(15);
const MAX_JSON_BYTES: usize = 2 * 1024 * 1024;
const MAX_IMAGE_BYTES: usize = 2 * 1024 * 1024;
/// 支付二维码地址最多跟几次跳转。
const MAX_IMAGE_REDIRECTS: usize = 3;

/// 页面能看懂的错误：`code` 决定下一步（例如 SESSION_EXPIRED 回到登录），`message` 直接显示。
#[derive(Debug)]
struct ApiError {
    message: String,
    status: u16,
    code: Value,
}

impl ApiError {
    fn new(message: impl Into<String>) -> Self {
        Self { message: message.into(), status: 0, code: json!("") }
    }

    fn code(message: impl Into<String>, code: &str) -> Self {
        Self { code: json!(code), ..Self::new(message) }
    }
}

type ApiResult<T> = Result<T, ApiError>;

/// 后台调用入口。`config` 是拓展的生效配置（读 `siteBase`），`data_dir` 是拓展数据目录。
/// 方法名不认识时返回 Err，和其它拓展后台「没有这个方法」的报错一致；其余情况都返回带 ok 的对象。
pub async fn call(config: &Value, data_dir: &Path, method: &str, params: &Value) -> anyhow::Result<Value> {
    let account = Account { site: normalize_site(config.get("siteBase")), session_file: data_dir.join("session.json") };
    let empty = Value::Object(Map::new());
    let params = if params.is_object() { params } else { &empty };
    let outcome = match method {
        "state" => Ok(account.state()),
        "login" => account.login(params).await,
        "logout" => {
            account.clear_session();
            Ok(json!({}))
        }
        "overview" => account.overview().await,
        "ensureKey" => account.ensure_key().await,
        "freeTrial" => account.post_message("/api/relay/free-trial", "已开通免费试用").await,
        "useResetCard" => account.post_message("/api/relay/reset-card/use", "已使用重置卡").await,
        "createOrder" => account.create_order(params).await,
        "orderStatus" => account.order_status(params).await,
        "openUrl" => open_url_param(params),
        "openSite" => account.open_site(params),
        _ => anyhow::bail!("后台没有方法 {method}"),
    };
    Ok(match outcome {
        Ok(mut result) => {
            let mut merged = Map::new();
            merged.insert("ok".into(), json!(true));
            if let Some(fields) = result.as_object_mut() {
                merged.append(fields);
            }
            Value::Object(merged)
        }
        Err(error) => json!({ "ok": false, "code": error.code, "status": error.status, "message": error.message }),
    })
}

struct Account {
    site: String,
    session_file: PathBuf,
}

impl Account {
    fn read_session(&self) -> Option<Value> {
        let session: Value = serde_json::from_slice(&std::fs::read(&self.session_file).ok()?).ok()?;
        session.get("token").and_then(Value::as_str).is_some_and(|token| !token.is_empty()).then_some(session)
    }

    fn write_session(&self, session: &Value) -> ApiResult<()> {
        let fail = |error: std::io::Error| ApiError::new(format!("保存登录状态失败：{error}"));
        if let Some(parent) = self.session_file.parent() {
            std::fs::create_dir_all(parent).map_err(fail)?;
        }
        std::fs::write(&self.session_file, session.to_string()).map_err(fail)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&self.session_file, std::fs::Permissions::from_mode(0o600));
        }
        Ok(())
    }

    fn clear_session(&self) {
        let _ = std::fs::remove_file(&self.session_file);
    }

    /// 调网站接口：带上登录令牌，401 当作登录过期并清掉本地令牌；`success: false` 转成 ApiError。
    async fn api(&self, method: reqwest::Method, path: &str, body: Option<Value>, auth: bool) -> ApiResult<Value> {
        let url = format!("{}{path}", self.site);
        let client = client_for(&url)?;
        let mut request = client.request(method, &url).header("Accept", "application/json");
        if auth {
            let session = self
                .read_session()
                .ok_or(ApiError { status: 401, ..ApiError::code("请先登录 imgenai 账号", "NOT_LOGGED_IN") })?;
            let token = session.get("token").and_then(Value::as_str).unwrap_or_default();
            request = request.header("Authorization", format!("Bearer {token}"));
        }
        if let Some(body) = body {
            request = request.header("Content-Type", "application/json").body(body.to_string());
        }
        let (status, _, bytes) = send(request, MAX_JSON_BYTES).await?;
        let json: Option<Value> = serde_json::from_slice(&bytes).ok();
        if auth && status == 401 {
            self.clear_session();
            return Err(ApiError { status: 401, ..ApiError::code("登录已过期，请重新登录", "SESSION_EXPIRED") });
        }
        let refused = json.as_ref().and_then(|json| json.get("success")) == Some(&Value::Bool(false));
        if !(200..300).contains(&status) || refused {
            let message = json
                .as_ref()
                .and_then(|json| text_of(json.get("message")))
                .unwrap_or_else(|| format!("imgenai 返回 HTTP {status}"));
            let code = json.as_ref().and_then(|json| json.get("code")).filter(|code| truthy(code)).cloned().unwrap_or(json!(""));
            return Err(ApiError { message, status, code });
        }
        match json {
            Some(json @ (Value::Object(_) | Value::Array(_))) => Ok(json),
            _ => Err(ApiError::new("imgenai 返回了无法识别的内容")),
        }
    }

    fn state(&self) -> Value {
        let session = self.read_session();
        json!({
            "loggedIn": session.is_some(),
            "user": session.as_ref().and_then(|session| session.get("user")).cloned().unwrap_or(Value::Null),
            "site": self.site,
        })
    }

    async fn login(&self, params: &Value) -> ApiResult<Value> {
        let phone = params.get("phone").and_then(text_value).unwrap_or_default().trim().to_string();
        let password = params.get("password").and_then(text_value).unwrap_or_default();
        if phone.is_empty() || password.is_empty() {
            return Err(ApiError::code("请填写手机号和密码", "INVALID_INPUT"));
        }
        let json = self
            .api(reqwest::Method::POST, "/api/auth/login", Some(json!({ "phone": phone, "password": password })), false)
            .await?;
        let token = json.get("token").and_then(Value::as_str).filter(|token| !token.is_empty());
        let Some(token) = token else {
            let message = text_of(json.get("message")).unwrap_or_else(|| "登录失败".into());
            return Err(ApiError::code(message, "LOGIN_FAILED"));
        };
        let user = pick_user(json.get("user"), &phone);
        self.write_session(&json!({ "token": token, "user": user, "savedAt": now_millis() }))?;
        Ok(json!({ "user": user }))
    }

    async fn overview(&self) -> ApiResult<Value> {
        let Some(session) = self.read_session() else {
            return Ok(json!({ "loggedIn": false }));
        };
        let sub = unwrap(self.api(reqwest::Method::GET, "/api/relay/subscription", None, true).await?);
        let sub = if sub.is_object() || sub.is_array() { sub } else { Value::Null };
        Ok(json!({ "loggedIn": true, "user": session.get("user").cloned().unwrap_or(Value::Null), "sub": sub }))
    }

    async fn list_tokens(&self) -> ApiResult<Vec<Value>> {
        let data = unwrap(self.api(reqwest::Method::GET, "/api/relay/tokens", None, true).await?);
        Ok(match data {
            Value::Array(list) => list,
            Value::Object(mut map) => match map.remove("items") {
                Some(Value::Array(list)) => list,
                _ => Vec::new(),
            },
            _ => Vec::new(),
        })
    }

    /// 取客户端专用密钥，没有就新建；没订阅时网站会拒绝新建（RELAY_NO_SUBSCRIPTION / RELAY_NOT_ENABLED）。
    async fn ensure_key(&self) -> ApiResult<Value> {
        let mut token = pick_token(self.list_tokens().await?);
        if token.is_none() {
            self.api(reqwest::Method::POST, "/api/relay/tokens", Some(json!({ "name": KEY_NAME })), true).await?;
            token = pick_token(self.list_tokens().await?);
        }
        let Some(key) = token else {
            return Err(ApiError::code("没有拿到中转密钥，请到官网「API 中转」页面查看", "KEY_UNAVAILABLE"));
        };
        Ok(json!({ "key": normalize_key(&key), "name": KEY_NAME }))
    }

    async fn post_message(&self, path: &str, fallback: &str) -> ApiResult<Value> {
        let json = self.api(reqwest::Method::POST, path, Some(json!({})), true).await?;
        Ok(json!({ "message": text_of(json.get("message")).unwrap_or_else(|| fallback.into()) }))
    }

    /// kind = subscription（plan 为 paid / pro）或 pack（加油包）。金额由网站后端按档位计算，这里不传价格。
    async fn create_order(&self, params: &Value) -> ApiResult<Value> {
        let body = if params.get("kind").and_then(Value::as_str) == Some("pack") {
            json!({ "type": "relay_pack" })
        } else {
            let plan = if params.get("plan").and_then(Value::as_str) == Some("pro") { "pro" } else { "paid" };
            json!({ "type": "subscription", "plan": plan })
        };
        let order = unwrap(self.api(reqwest::Method::POST, "/api/payment/create", Some(body), true).await?);
        let Some(trade_order_id) = text_of(order.get("tradeOrderId")) else {
            return Err(ApiError::code("创建订单失败，请稍后重试", "ORDER_FAILED"));
        };
        let qr_url = text_of(order.get("qrcodeUrl")).unwrap_or_default();
        let payment_url = text_of(order.get("paymentUrl")).unwrap_or_else(|| qr_url.clone());
        let qr_data_url = image_data_url(&qr_url).await;
        Ok(json!({ "tradeOrderId": trade_order_id, "paymentUrl": payment_url, "qrDataUrl": qr_data_url }))
    }

    async fn order_status(&self, params: &Value) -> ApiResult<Value> {
        let id = params.get("tradeOrderId").and_then(text_value).unwrap_or_default();
        let valid = (1..=80).contains(&id.len()) && id.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-');
        if !valid {
            return Err(ApiError::code("订单号不合法", "INVALID_INPUT"));
        }
        let json = self.api(reqwest::Method::GET, &format!("/api/payment/status/{id}"), None, true).await?;
        let status = text_of(json.get("data").and_then(|data| data.get("status")))
            .or_else(|| text_of(json.get("status")))
            .unwrap_or_else(|| "pending".into());
        Ok(json!({ "status": status }))
    }

    /// 打开官网的某个页面（注册、找回密码、财务中心等都在官网完成）。
    fn open_site(&self, params: &Value) -> ApiResult<Value> {
        let hash = params.get("hash").and_then(text_value).unwrap_or_default();
        let valid = hash.len() <= 40 && hash.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-');
        let suffix = if valid && !hash.is_empty() { format!("/#{hash}") } else { "/".into() };
        open_external(&format!("{}{suffix}", self.site))?;
        Ok(json!({}))
    }
}

/// 在系统浏览器里打开：只接受 http(s) 地址。
fn open_url_param(params: &Value) -> ApiResult<Value> {
    let url = params.get("url").and_then(text_value).unwrap_or_default();
    if !is_http_url(&url) {
        return Err(ApiError::code("只能打开网页地址", "INVALID_INPUT"));
    }
    open_external(&url)?;
    Ok(json!({}))
}

fn open_external(url: &str) -> ApiResult<()> {
    #[cfg(windows)]
    let result = crate::windows_integration::open_url(url);
    #[cfg(not(windows))]
    let result = {
        let program = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
        std::process::Command::new(program)
            .arg(url)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map(|mut child| {
                // 打开命令很快就退出；后台收掉，不留僵尸进程。
                std::thread::spawn(move || child.wait());
            })
            .map_err(anyhow::Error::from)
    };
    result.map_err(|error| ApiError::new(format!("无法打开浏览器：{error:#}")))
}

/// 支付二维码转成 data URL（管理器面板的 CSP 不放行外部图片）。支付平台的二维码地址会先 302 到
/// 真正的图片，跟最多 3 次跳转。拿不到时返回空串，页面改为显示「在浏览器中打开」。
async fn image_data_url(url: &str) -> String {
    let mut current = url.to_string();
    for _ in 0..=MAX_IMAGE_REDIRECTS {
        if !is_http_url(&current) {
            return String::new();
        }
        let Ok(client) = client_for(&current) else {
            return String::new();
        };
        let Ok((status, headers, bytes)) = send(client.get(&current).header("Accept", "image/*"), MAX_IMAGE_BYTES).await else {
            return String::new();
        };
        let location = headers.get(reqwest::header::LOCATION).and_then(|value| value.to_str().ok());
        let next = location.and_then(|location| reqwest::Url::parse(&current).ok()?.join(location).ok());
        if let (true, Some(next)) = ([301, 302, 303, 307, 308].contains(&status), next) {
            current = next.to_string();
            continue;
        }
        let kind = headers
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.split(';').next())
            .unwrap_or_default()
            .trim()
            .to_string();
        if status != 200 || !kind.starts_with("image/") {
            return String::new();
        }
        return format!("data:{kind};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes));
    }
    String::new()
}

/// 不自动跟随跳转：接口的跳转原样当错误处理，二维码的跳转由 [`image_data_url`] 自己跟。
fn client_for(url: &str) -> ApiResult<reqwest::Client> {
    if reqwest::Url::parse(url).is_err() {
        return Err(ApiError::new(format!("地址不合法：{url}")));
    }
    crate::http_client::proxied_client_for_url_with_redirect(USER_AGENT, url, TIMEOUT, TIMEOUT, reqwest::redirect::Policy::none())
        .map_err(|error| ApiError::code(format!("连接 imgenai 失败：{error:#}"), "NETWORK"))
}

/// 发请求并读完响应体；超时、超长都按错误处理。
async fn send(request: reqwest::RequestBuilder, max_bytes: usize) -> ApiResult<(u16, reqwest::header::HeaderMap, Vec<u8>)> {
    let network = |error: reqwest::Error| {
        if error.is_timeout() {
            ApiError::code("连接 imgenai 超时，请检查网络后重试", "TIMEOUT")
        } else {
            ApiError::code(format!("连接 imgenai 失败：{}", describe(&error)), "NETWORK")
        }
    };
    let mut response = request.send().await.map_err(network)?;
    let status = response.status().as_u16();
    let headers = response.headers().clone();
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(network)? {
        if body.len() + chunk.len() > max_bytes {
            return Err(ApiError::new("imgenai 返回的内容过大"));
        }
        body.extend_from_slice(&chunk);
    }
    Ok((status, headers, body))
}

/// reqwest 的错误只说「error sending request」，真正原因（DNS、证书、连接被拒）在 source 链里。
fn describe(error: &reqwest::Error) -> String {
    let mut text = error.to_string();
    let mut source = std::error::Error::source(error);
    while let Some(cause) = source {
        text.push('：');
        text.push_str(&cause.to_string());
        source = cause.source();
    }
    text
}

fn normalize_site(value: Option<&Value>) -> String {
    let text = value.and_then(Value::as_str).unwrap_or_default().trim().trim_end_matches('/').to_string();
    let host_ok = text
        .split_once("://")
        .filter(|(scheme, _)| scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https"))
        .and_then(|(_, rest)| rest.chars().next())
        .is_some_and(|first| first != '/' && !first.is_whitespace());
    if host_ok && !text.chars().any(char::is_whitespace) { text } else { DEFAULT_SITE.into() }
}

fn is_http_url(text: &str) -> bool {
    let lower = text.get(..8).unwrap_or(text).to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

/// 网站接口有的把结果放在 data 里，有的直接平铺；两种都认。
fn unwrap(json: Value) -> Value {
    match json {
        Value::Object(mut map) if map.contains_key("data") => map.remove("data").unwrap_or(Value::Null),
        other => other,
    }
}

/// 只留界面要用的用户字段，别把网站返回的其它信息落盘。
fn pick_user(user: Option<&Value>, phone: &str) -> Value {
    let field = |names: &[&str]| names.iter().find_map(|name| text_of(user.and_then(|user| user.get(*name))));
    json!({
        "id": user.and_then(|user| user.get("id")).cloned().unwrap_or(Value::Null),
        "phone": field(&["phone"]).unwrap_or_else(|| phone.to_string()),
        "nickname": field(&["nickname", "username", "name"]).unwrap_or_default(),
    })
}

/// 列表里能直接用的密钥：名字对得上，且不是打码后的（含 * 的说明网站只给了掩码）。
fn pick_token(list: Vec<Value>) -> Option<String> {
    list.into_iter().find_map(|token| {
        let key = token.get("key").and_then(Value::as_str)?;
        (token.get("name").and_then(Value::as_str) == Some(KEY_NAME) && !key.is_empty() && !key.contains('*')).then(|| key.to_string())
    })
}

fn normalize_key(key: &str) -> String {
    let text = key.trim();
    if text.is_empty() || text.starts_with("sk-") { text.to_string() } else { format!("sk-{text}") }
}

/// 字符串或数字转成文字；空串、0、false、null 当作没有（与页面原来的取值习惯一致）。
fn text_of(value: Option<&Value>) -> Option<String> {
    value.filter(|value| truthy(value)).and_then(text_value)
}

fn text_value(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(flag) => Some(flag.to_string()),
        _ => None,
    }
}

fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(flag) => *flag,
        Value::Number(number) => number.as_f64().is_some_and(|number| number != 0.0),
        Value::String(text) => !text.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|time| time.as_millis() as u64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn site_falls_back_to_default() {
        assert_eq!(normalize_site(Some(&json!("https://imgenai.net/"))), "https://imgenai.net");
        assert_eq!(normalize_site(Some(&json!(" http://127.0.0.1:9000// "))), "http://127.0.0.1:9000");
        assert_eq!(normalize_site(Some(&json!("ftp://x"))), DEFAULT_SITE);
        assert_eq!(normalize_site(Some(&json!("https:///path"))), DEFAULT_SITE);
        assert_eq!(normalize_site(None), DEFAULT_SITE);
    }

    #[test]
    fn token_must_be_named_and_unmasked() {
        let list = vec![
            json!({ "name": KEY_NAME, "key": "sk-ab**cd" }),
            json!({ "name": "别的", "key": "sk-other" }),
            json!({ "name": KEY_NAME, "key": "plain" }),
        ];
        assert_eq!(pick_token(list).as_deref(), Some("plain"));
        assert_eq!(normalize_key("plain"), "sk-plain");
        assert_eq!(normalize_key("sk-x"), "sk-x");
    }

    #[test]
    fn user_keeps_only_display_fields() {
        let user = pick_user(Some(&json!({ "id": 7, "username": "u", "email": "secret" })), "138");
        assert_eq!(user, json!({ "id": 7, "phone": "138", "nickname": "u" }));
        assert_eq!(pick_user(None, "139"), json!({ "id": null, "phone": "139", "nickname": "" }));
    }

    #[test]
    fn unwrap_reads_data_or_flat() {
        assert_eq!(unwrap(json!({ "data": { "a": 1 } })), json!({ "a": 1 }));
        assert_eq!(unwrap(json!({ "data": null })), Value::Null);
        assert_eq!(unwrap(json!({ "a": 1 })), json!({ "a": 1 }));
    }
}
