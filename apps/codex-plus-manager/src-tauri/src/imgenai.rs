//! imgenai 分发版专有的启动 / 退出步骤，集中在这里，合并上游时不和上游的启动流程搅在一起。
//!
//! - 首次启动：默认开机启动、登录时隐藏到托盘（只设一次，用户之后关掉就不再打开）。
//! - 打开「原生 Fast 强制解锁」（每台电脑一次，含旧版升级上来的）：imgenai 的模型都支持 Fast，
//!   纯 API Key 登录的 Codex 默认不显示 Fast 按钮。
//! - 启动时：上次退出切成直连 imgenai 的 Codex 配置改回网关（必须在网关接入、自动接管、启动 Codex 之前）。
//! - 退出时：Codex 还连着本机网关的话改成直连 imgenai，客户端没开时 Codex 照样能用。
//! - 启动时（Windows）：当前用户的 NO_PROXY 补上回环地址，开着代理时 Codex 发往本机网关的请求不被交给代理
//!   （见 core::imgenai_loopback_proxy）。

use codex_plus_core::bundled_extensions::IMGENAI_ACCOUNT_KEY;
use codex_plus_core::imgenai_codex_direct;
use serde_json::json;

/// 首次启动默认值已经设过的标记。
const DEFAULTS_MARKER: &str = "imgenai-defaults-v1";

fn log(event: &str, detail: serde_json::Value) {
    let _ = codex_plus_core::diagnostic_log::append_diagnostic_log(event, detail);
}

/// 首次启动（含从旧版升级上来的第一次）：打开开机启动，并在登录启动时隐藏到托盘。
/// 网关在客户端进程里，客户端常驻 Codex 才连得上；设成功才写标记，失败的下次启动再试。
pub fn apply_first_run_defaults() {
    let marker = codex_plus_core::paths::default_app_state_dir().join(DEFAULTS_MARKER);
    if marker.exists() {
        return;
    }
    let result = crate::commands::set_watcher_enabled(true);
    if result.status != "ok" {
        log("imgenai.first_run_autostart_failed", json!({ "message": result.message }));
        return;
    }
    if let Err(error) = codex_plus_core::watcher::set_watcher_start_minimized(true) {
        log("imgenai.first_run_minimized_failed", json!({ "message": error.to_string() }));
    }
    if let Some(parent) = marker.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&marker, b"autostart+minimized\n");
    log("imgenai.first_run_defaults", json!({ "autostart": true, "startMinimized": true }));
}

/// 「原生 Fast 强制解锁」已经替用户打开过的标记。
const FAST_DEFAULT_MARKER: &str = "imgenai-fast-default-v1";

/// imgenai 的模型都支持 Fast（服务端按 2.5 倍计费）：纯 API Key 登录的 Codex 默认不显示 Fast 按钮，
/// 这里给每台电脑打开一次「原生 Fast 强制解锁」，下次启动 Codex 时生效。用户之后关掉就不再打开；
/// 设成功才写标记，失败的下次启动再试。
pub fn enable_native_fast_once() {
    let marker = codex_plus_core::paths::default_app_state_dir().join(FAST_DEFAULT_MARKER);
    if marker.exists() {
        return;
    }
    let store = codex_plus_core::settings::SettingsStore::default();
    if let Err(error) = store.update(json!({ "codexAppNativeFastUnlock": true })) {
        log("imgenai.native_fast_default_failed", json!({ "message": error.to_string() }));
        return;
    }
    if let Some(parent) = marker.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&marker, b"codexAppNativeFastUnlock\n");
    log("imgenai.native_fast_default", json!({ "codexAppNativeFastUnlock": true }));
}

/// 启动时最先调用：退出时切成直连 imgenai 的 Codex 配置改回网关。
pub fn switch_codex_back_on_startup() {
    let codex_home = codex_plus_core::gateway::apply::ApplyContext::default_paths(0).codex_home;
    match imgenai_codex_direct::switch_back(&codex_home, &codex_plus_core::paths::default_app_state_dir()) {
        Ok(true) => log("imgenai.codex_switched_back", json!({})),
        Ok(false) => {}
        Err(error) => log("imgenai.codex_switch_back_failed", json!({ "message": format!("{error:#}") })),
    }
}

/// 退出时调用（网关自己的「退出时恢复直连」之后，以及事件循环最终退出时）：Codex 还连着本机网关就改成
/// 直连 imgenai。可以重复调用：切过一次或 Codex 已不连网关时什么也不做。
pub fn switch_codex_direct_on_exit() {
    // 与网关接入时写进 Codex 配置的端口取法一致（常驻 helper 的端口，没有时用默认端口）。
    let gateway_port = codex_plus_core::resident_helper::resident_helper_port()
        .unwrap_or_else(codex_plus_core::protocol_proxy::protocol_proxy_port);
    let api_key = crate::commands::default_user_script_manager()
        .enabled_extension_backend(IMGENAI_ACCOUNT_KEY)
        .ok()
        .and_then(|(_, _, config)| config.get("apiKey").and_then(|value| value.as_str()).map(str::to_string))
        .unwrap_or_default();
    let codex_home = codex_plus_core::gateway::apply::ApplyContext::default_paths(gateway_port).codex_home;
    match imgenai_codex_direct::switch_to_direct(
        &codex_home,
        &codex_plus_core::paths::default_app_state_dir(),
        gateway_port,
        &api_key,
    ) {
        Ok(true) => log("imgenai.codex_switched_direct", json!({ "port": gateway_port })),
        Ok(false) => log("imgenai.codex_direct_skipped", json!({ "port": gateway_port, "loggedIn": !api_key.is_empty() })),
        Err(error) => log("imgenai.codex_switch_direct_failed", json!({ "message": format!("{error:#}") })),
    }
}

/// Windows：当前用户的 NO_PROXY 缺回环地址时补上（每次启动核对一次，已经有了就不动）。
/// 写了之后已经在运行的 Codex 要重启一次才生效。其它系统不需要：macOS 系统代理的例外列表本来就是 127.0.0.1 这种写法。
pub fn ensure_loopback_bypasses_proxy() {
    #[cfg(windows)]
    match codex_plus_core::imgenai_loopback_proxy::ensure_windows_user_no_proxy() {
        Ok(true) => log("imgenai.no_proxy_loopback_added", json!({})),
        Ok(false) => {}
        Err(error) => log("imgenai.no_proxy_loopback_failed", json!({ "message": format!("{error:#}") })),
    }
}
