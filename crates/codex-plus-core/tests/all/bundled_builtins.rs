//! 打字特效、背景图从内置功能改成了官方拓展包（来源 `bundled`）：包本身、旧设置迁移、背景图的读图路由。
use codex_plus_core::bundled_extensions::{
    IMAGE_OVERLAY_KEY, TYPING_EFFECTS_KEY, image_overlay_files, image_overlay_image, materialize, typing_effects_files,
};
use codex_plus_core::settings::BackendSettings;
use codex_plus_core::user_scripts::UserScriptManager;
use serde_json::{Map, Value, json};

fn manager(root: &std::path::Path) -> UserScriptManager {
    UserScriptManager::new(root.join("builtin"), root.join("user"), root.join("user_scripts.json"))
        .with_bundled_dir(root.join("bundled"))
}

fn script(manager: &UserScriptManager, key: &str) -> Value {
    manager.inventory().unwrap()["scripts"].as_array().unwrap().iter().find(|item| item["key"] == key).cloned().unwrap()
}

fn ready(root: &std::path::Path) -> UserScriptManager {
    materialize(&root.join("bundled")).unwrap();
    manager(root)
}

#[test]
fn both_packages_are_valid_extensions_that_only_use_public_interfaces() {
    let temp = tempfile::tempdir().unwrap();
    let manager = ready(temp.path());
    for (key, id) in [(TYPING_EFFECTS_KEY, "typing-effects"), (IMAGE_OVERLAY_KEY, "image-overlay")] {
        let item = script(&manager, key);
        assert_eq!(item["error"], "", "{item}");
        assert_eq!(item["source"], "bundled");
        assert_eq!(item["kind"], "package");
        assert_eq!(item["package"]["id"], id);
    }
    // 背景图声明的路由都在拓展路由白名单里；打字特效不用任何后端路由。
    let api = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/inject/renderer-inject/91-extension-api.js")).unwrap();
    let overlay = script(&manager, IMAGE_OVERLAY_KEY);
    for route in overlay["package"]["routes"].as_array().unwrap() {
        assert!(api.contains(&format!("\"{}\"", route.as_str().unwrap())), "{route} 不在拓展路由白名单里");
    }
    assert!(script(&manager, TYPING_EFFECTS_KEY)["package"]["routes"].as_array().is_none_or(Vec::is_empty));
    // 页面脚本只用公开接口：不碰注入层的私有函数与全局变量。
    for (path, content) in typing_effects_files().into_iter().chain(image_overlay_files()) {
        if !path.ends_with(".js") {
            continue;
        }
        let source = String::from_utf8(content).unwrap();
        for private in [
            "postJson(",
            "codexPlusSettings(",
            "codexPlusBackendSettings",
            "__codexPlusBuiltinInactive",
            "registerCodexPlusExtensionSelector",
            "sendCodexPlusDiagnostic",
            "isExtensionUiNode",
            "visibleElement(",
            "runScanStep",
            "__CODEX_PLUS_",
        ] {
            assert!(!source.contains(private), "{path} 还在用 {private}");
        }
    }
}

#[test]
fn image_path_is_a_file_field_with_an_image_filter() {
    let temp = tempfile::tempdir().unwrap();
    let manager = ready(temp.path());
    let schema = script(&manager, IMAGE_OVERLAY_KEY)["package"]["configSchema"].clone();
    let field = schema.as_array().unwrap().iter().find(|field| field["key"] == "imagePath").cloned().unwrap();
    assert_eq!(field["format"], "file");
    assert_eq!(field["extensions"], json!(["png", "jpg", "jpeg", "webp", "gif", "bmp"]));
    // 没写 format 的字段不带这两个键。
    let opacity = schema.as_array().unwrap().iter().find(|field| field["key"] == "opacity").cloned().unwrap();
    assert!(opacity.get("format").is_none() && opacity.get("extensions").is_none());
}

#[test]
fn file_format_is_only_accepted_on_string_fields() {
    let temp = tempfile::tempdir().unwrap();
    let manager = ready(temp.path());
    let write = |id: &str, config: Value| {
        let dir = temp.path().join("user").join(id);
        std::fs::create_dir_all(&dir).unwrap();
        let manifest = json!({ "id": id, "renderer": ["main.js"], "engines": { "codexpp": 1 }, "config": config });
        std::fs::write(dir.join("codexpp.json"), manifest.to_string()).unwrap();
        std::fs::write(dir.join("main.js"), "").unwrap();
    };
    write("ok-file", json!({ "path": { "type": "string", "format": "file", "extensions": ["txt"] } }));
    write("bad-kind", json!({ "count": { "type": "number", "format": "file" } }));
    write("bad-format", json!({ "path": { "type": "string", "format": "folder" } }));
    write("bad-extension", json!({ "path": { "type": "string", "format": "file", "extensions": [".png"] } }));
    write("orphan-extensions", json!({ "path": { "type": "string", "extensions": ["png"] } }));
    assert_eq!(script(&manager, "user:ok-file")["error"], "");
    for id in ["bad-kind", "bad-format", "bad-extension", "orphan-extensions"] {
        let error = script(&manager, &format!("user:{id}"))["error"].as_str().unwrap().to_string();
        assert!(error.contains("format") || error.contains("extensions"), "{id}: {error}");
    }
}

#[test]
fn old_typing_effect_setting_migrates_once() {
    let temp = tempfile::tempdir().unwrap();
    let manager = ready(temp.path());
    let settings = BackendSettings { codex_app_typing_effect: "stars".into(), ..BackendSettings::default() };
    manager.migrate_bundled_builtins(&settings).unwrap();
    let item = script(&manager, TYPING_EFFECTS_KEY);
    assert_eq!(item["enabled"], true);
    assert_eq!(item["package"]["config"]["effect"], "stars");
    // 只迁移一次：之后在拓展里改的启停、配置不会被旧设置盖掉。
    manager.set_script_enabled(TYPING_EFFECTS_KEY, false).unwrap();
    manager.migrate_bundled_builtins(&settings).unwrap();
    assert_eq!(script(&manager, TYPING_EFFECTS_KEY)["enabled"], false);
}

#[test]
fn typing_effects_stay_off_for_users_who_never_picked_one() {
    let temp = tempfile::tempdir().unwrap();
    let manager = ready(temp.path());
    manager.migrate_bundled_builtins(&BackendSettings::default()).unwrap();
    let item = script(&manager, TYPING_EFFECTS_KEY);
    assert_eq!(item["enabled"], false);
    assert_eq!(item["package"]["config"]["effect"], "rainbow", "启用后默认彩虹");
}

#[test]
fn old_image_overlay_settings_migrate_and_need_an_image_to_be_enabled() {
    let temp = tempfile::tempdir().unwrap();
    let manager = ready(temp.path());
    let settings = BackendSettings {
        codex_app_image_overlay_enabled: true,
        codex_app_image_overlay_path: "  /Users/me/Pictures/wall.png ".into(),
        codex_app_image_overlay_opacity: 60,
        codex_app_image_overlay_fit_mode: "tile".into(),
        ..BackendSettings::default()
    };
    manager.migrate_bundled_builtins(&settings).unwrap();
    let item = script(&manager, IMAGE_OVERLAY_KEY);
    assert_eq!(item["enabled"], true);
    assert_eq!(item["package"]["config"], json!({ "imagePath": "/Users/me/Pictures/wall.png", "opacity": 60, "fitMode": "tile" }));

    // 开着但没选图片：迁移成停用。
    let other = tempfile::tempdir().unwrap();
    let manager = ready(other.path());
    let settings = BackendSettings { codex_app_image_overlay_enabled: true, ..BackendSettings::default() };
    manager.migrate_bundled_builtins(&settings).unwrap();
    assert_eq!(script(&manager, IMAGE_OVERLAY_KEY)["enabled"], false);
    assert_eq!(script(&manager, IMAGE_OVERLAY_KEY)["package"]["config"]["fitMode"], "fit");
}

#[test]
fn image_route_reads_only_the_path_saved_in_the_extension_config() {
    let temp = tempfile::tempdir().unwrap();
    let manager = ready(temp.path());
    let save = |path: &str| {
        let mut values = Map::new();
        values.insert("imagePath".into(), json!(path));
        manager.set_extension_config(IMAGE_OVERLAY_KEY, &values).unwrap();
    };
    assert_eq!(image_overlay_image(&manager)["status"], "empty");

    let png = temp.path().join("wall.png");
    std::fs::write(&png, [0x89, b'P', b'N', b'G']).unwrap();
    save(&png.to_string_lossy());
    let result = image_overlay_image(&manager);
    assert_eq!(result["status"], "ok");
    assert_eq!(result["dataUrl"], "data:image/png;base64,iVBORw==");

    save(&temp.path().join("missing.png").to_string_lossy());
    assert_eq!(image_overlay_image(&manager)["status"], "failed");
    let text = temp.path().join("notes.txt");
    std::fs::write(&text, "secret").unwrap();
    save(&text.to_string_lossy());
    let result = image_overlay_image(&manager);
    assert_eq!(result["status"], "failed", "不是图片就不读");
    assert!(result.get("dataUrl").is_none());
}

#[test]
fn image_route_is_declared_only_while_the_extension_is_enabled() {
    let temp = tempfile::tempdir().unwrap();
    let manager = ready(temp.path());
    manager.migrate_bundled_builtins(&BackendSettings::default()).unwrap();
    assert!(!manager.route_declared("/image-overlay/image"));
    manager.set_script_enabled(IMAGE_OVERLAY_KEY, true).unwrap();
    assert!(manager.route_declared("/image-overlay/image"));
}

// ---- 用量状态栏、请求状态条：也从内置拓展改成了官方拓展包 ----

use codex_plus_core::bundled_extensions::{
    PROXY_REQUEST_STATUS_KEY, USAGE_STATUSLINE_KEY, proxy_request_status_files, usage_statusline_files,
};

#[test]
fn status_bar_packages_are_valid_and_only_use_public_interfaces() {
    let temp = tempfile::tempdir().unwrap();
    let manager = ready(temp.path());
    let api = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/inject/renderer-inject/91-extension-api.js")).unwrap();
    for (key, id, route) in [
        (USAGE_STATUSLINE_KEY, "usage-statusline", "/thread-usage-live"),
        (PROXY_REQUEST_STATUS_KEY, "proxy-request-status", "/proxy-status"),
    ] {
        let item = script(&manager, key);
        assert_eq!(item["error"], "", "{item}");
        assert_eq!(item["source"], "bundled");
        assert_eq!(item["package"]["id"], id);
        let routes = item["package"]["routes"].as_array().unwrap();
        assert!(routes.iter().any(|item| item == route), "{id} 要在清单里声明 {route}");
        // 声明的路由都在拓展路由白名单里。
        for route in routes {
            assert!(api.contains(&format!("\"{}\"", route.as_str().unwrap())), "{route} 不在拓展路由白名单里");
        }
    }
    for (path, content) in usage_statusline_files().into_iter().chain(proxy_request_status_files()) {
        if !path.ends_with(".js") {
            continue;
        }
        let source = String::from_utf8(content).unwrap();
        for private in [
            "postJson(",
            "codexPlusSettings(",
            "codexPlusBackendSettings",
            "__codexPlusBuiltinInactive",
            "registerCodexPlusExtensionSelector",
            "sendCodexPlusDiagnostic",
            "recordCodexPlusBridgeFailure",
            "currentSessionRef",
            "visibleElement(",
            "runScanStep",
            "__codexSessionDeleteBridge",
            "__CODEX_PLUS_",
        ] {
            assert!(!source.contains(private), "{path} 还在用 {private}");
        }
    }
    // 不再是内置拓展：设置清单与内置拓展目录里都没有它们。
    for id in ["usage-statusline", "proxy-request-status"] {
        assert!(codex_plus_core::settings_catalog::builtin_extension_def(id).is_none(), "{id} 还在内置拓展目录里");
    }
}

#[test]
fn old_status_bar_switches_migrate_once() {
    // 默认都是开的：老用户升级后照旧显示。
    let temp = tempfile::tempdir().unwrap();
    let manager = ready(temp.path());
    manager.migrate_bundled_builtins(&BackendSettings::default()).unwrap();
    assert_eq!(script(&manager, USAGE_STATUSLINE_KEY)["enabled"], true);
    assert_eq!(script(&manager, PROXY_REQUEST_STATUS_KEY)["enabled"], true);

    // 原来关掉的：迁移成停用；迁移过之后改旧字段不再影响。
    let other = tempfile::tempdir().unwrap();
    let manager = ready(other.path());
    let off = BackendSettings {
        codex_app_usage_status_line: false,
        codex_app_proxy_request_status: false,
        ..BackendSettings::default()
    };
    manager.migrate_bundled_builtins(&off).unwrap();
    assert_eq!(script(&manager, USAGE_STATUSLINE_KEY)["enabled"], false);
    assert_eq!(script(&manager, PROXY_REQUEST_STATUS_KEY)["enabled"], false);
    manager.migrate_bundled_builtins(&BackendSettings::default()).unwrap();
    assert_eq!(script(&manager, USAGE_STATUSLINE_KEY)["enabled"], false, "迁移只做一次");
    // 停用时不声明路由，Codex++ 不替它干活。
    assert!(!manager.route_declared("/thread-usage-live"));
    manager.set_script_enabled(USAGE_STATUSLINE_KEY, true).unwrap();
    assert!(manager.route_declared("/thread-usage-live"));
}

#[test]
fn replacing_a_retired_builtin_id_makes_the_bundled_package_yield() {
    let temp = tempfile::tempdir().unwrap();
    let manager = ready(temp.path());
    manager.migrate_bundled_builtins(&BackendSettings::default()).unwrap();
    // 之前按文档写的 replaces: ["usage-statusline"] 仍然合法。
    let dir = temp.path().join("user").join("my-bar");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("codexpp.json"),
        json!({ "id": "my-bar", "renderer": "main.js", "replaces": ["usage-statusline"] }).to_string(),
    )
    .unwrap();
    std::fs::write(dir.join("main.js"), "").unwrap();

    let mine = script(&manager, "user:my-bar");
    assert_eq!(mine["error"], "", "{mine}");
    assert_eq!(mine["package"]["replaces"], json!(["usage-statusline"]));
    let bundled = script(&manager, USAGE_STATUSLINE_KEY);
    assert_eq!(bundled["enabled"], false, "顶替它的拓展启用时官方拓展包让位");
    assert_eq!(bundled["status"], "disabled");
    assert_eq!(bundled["replacedBy"], "user:my-bar");
    assert_eq!(script(&manager, PROXY_REQUEST_STATUS_KEY)["enabled"], true, "没被顶替的不受影响");

    // 顶替它的拓展停用后，官方拓展包回来。
    manager.set_script_enabled("user:my-bar", false).unwrap();
    let bundled = script(&manager, USAGE_STATUSLINE_KEY);
    assert_eq!(bundled["enabled"], true);
    assert!(bundled["replacedBy"].is_null());
}

// ---- imgenai 分发版：账户拓展包 ----

use codex_plus_core::bundled_extensions::{IMGENAI_ACCOUNT_KEY, imgenai_account_files};

#[test]
fn imgenai_account_package_is_valid_and_enabled_by_default() {
    let temp = tempfile::tempdir().unwrap();
    let manager = ready(temp.path());
    manager.migrate_bundled_builtins(&BackendSettings::default()).unwrap();
    let item = script(&manager, IMGENAI_ACCOUNT_KEY);
    assert_eq!(item["error"], "", "{item}");
    assert_eq!(item["source"], "bundled");
    assert_eq!(item["package"]["id"], "imgenai-account");
    assert_eq!(item["enabled"], true, "账户拓展默认启用");

    // 迁移只做一次：用户停用后再迁移不会被打开。
    manager.set_script_enabled(IMGENAI_ACCOUNT_KEY, false).unwrap();
    manager.migrate_bundled_builtins(&BackendSettings::default()).unwrap();
    assert_eq!(script(&manager, IMGENAI_ACCOUNT_KEY)["enabled"], false);
}

#[test]
fn imgenai_provider_uses_the_key_saved_after_login() {
    let temp = tempfile::tempdir().unwrap();
    let manager = ready(temp.path());
    manager.migrate_bundled_builtins(&BackendSettings::default()).unwrap();

    let provider = |manager: &UserScriptManager| {
        manager
            .enabled_extension_providers()
            .unwrap()
            .into_iter()
            .find(|(extension, provider)| extension == "imgenai-account" && provider.id == "imgenai")
            .map(|(_, provider)| provider)
    };
    // 没登录（没有密钥）时不并入网关，Codex 里不会列出一排必然 401 的模型。
    assert!(provider(&manager).is_none(), "没登录时不并入网关");

    // 管理器页面登录后经 config.set 写入 apiKey，供应商立即并入并用上这把密钥。
    let mut values = Map::new();
    values.insert("apiKey".into(), json!("sk-test-key"));
    manager.update_extension_config(IMGENAI_ACCOUNT_KEY, &values).unwrap();
    let after = provider(&manager).expect("登录后并入网关");
    assert_eq!(after.api_key, "sk-test-key");
    assert_eq!(after.base_url, "https://api.imgenai.net/v1");
    assert_eq!(after.protocol, "responses");
    assert!(after.models.iter().any(|model| model == "gpt-6.1-sol"));

    // 退出登录清空密钥后又不并入了。
    values.insert("apiKey".into(), json!(""));
    manager.update_extension_config(IMGENAI_ACCOUNT_KEY, &values).unwrap();
    assert!(provider(&manager).is_none(), "退出登录后不再并入");
    values.insert("apiKey".into(), json!("sk-test-key"));
    manager.update_extension_config(IMGENAI_ACCOUNT_KEY, &values).unwrap();

    // 停用拓展后供应商消失。
    manager.set_script_enabled(IMGENAI_ACCOUNT_KEY, false).unwrap();
    assert!(manager.enabled_extension_providers().unwrap().iter().all(|(extension, _)| extension != "imgenai-account"));
}

/// 登录后 imgenai 写进供应商池（管理器里看得到、接入时选得到模型）；换 Key 只换 Key；退出登录、停用拓展后移出。
#[test]
fn imgenai_provider_is_written_into_the_pool_after_login() {
    use codex_plus_core::gateway::apply::{self, ProviderInput};
    use codex_plus_core::gateway::extension_pool::{self, PoolSync};
    use codex_plus_core::gateway::store::GatewayStore;

    let temp = tempfile::tempdir().unwrap();
    let manager = ready(temp.path());
    manager.migrate_bundled_builtins(&BackendSettings::default()).unwrap();
    let store = GatewayStore::at(temp.path().join("gateway.json"));
    let source = "extension-pool:imgenai-account/imgenai";
    let set_key = |key: &str| {
        let mut values = Map::new();
        values.insert("apiKey".into(), json!(key));
        manager.update_extension_config(IMGENAI_ACCOUNT_KEY, &values).unwrap();
    };
    let pooled = || store.load().unwrap().providers.into_iter().filter(|entry| entry.source == source).collect::<Vec<_>>();
    let key_of = |relay: &codex_plus_core::settings::RelayProfile| {
        relay.api_keys.first().map(|entry| entry.api_key.clone()).unwrap_or_else(|| relay.api_key.clone())
    };

    // 没登录：什么都不写。
    assert_eq!(extension_pool::sync(&store, &manager).unwrap(), PoolSync::default());
    assert!(store.load().unwrap().providers.is_empty());

    // 登录拿到 Key：写进池里，id 就是 imgenai（Codex 里的模型名 imgenai/…），声明了 discoverModels 的交给调用方去拉模型。
    set_key("sk-one");
    let first = extension_pool::sync(&store, &manager).unwrap();
    assert!(first.changed);
    assert_eq!(first.created, vec!["imgenai".to_string()]);
    let entry = pooled().pop().expect("登录后写进供应商池");
    assert_eq!(entry.relay.id, "imgenai");
    assert_eq!(entry.relay.name, "imgenai");
    assert_eq!(entry.relay.base_url, "https://api.imgenai.net/v1");
    assert_eq!(key_of(&entry.relay), "sk-one");
    assert!(entry.relay.model_list.lines().any(|model| model == "gpt-6.1-sol"));
    assert!(entry.enabled);
    // 价格按 imgenai 服务端实扣（官方价 ×1.75，缓存/输出比例同服务端），用量页的费用与网页一致。
    let astra = &entry.model_prices["gpt-6-astra"];
    assert_eq!((astra.input, astra.cached_input, astra.output), (17.5, 1.75, 87.5));
    assert_eq!(entry.model_prices["gpt-6.1-sol"].cached_input, 0.175, "6.1 系缓存是输入的 1/20");
    assert_eq!(entry.model_prices["gpt-6"], entry.model_prices["gpt-6-astra"], "别名 gpt-6 → astra 同价");
    assert_eq!(entry.model_prices["gpt-5.6"], entry.model_prices["gpt-5.6-sol"], "别名 gpt-5.6 → 5.6-sol 同价");
    // 对账：11-16 实测 astra 一次（输入 25178、其中缓存 3712、输出 14）服务端扣 $0.3834。
    let cost = ((25178.0 - 3712.0) * astra.input + 3712.0 * astra.cached_input + 14.0 * astra.output) / 1e6;
    assert!((cost - 0.383376).abs() < 1e-9, "{cost}");

    // 再同步一次不重复写；路由时池里的这份顶掉拓展自己那份，不会出现两个 imgenai。
    assert_eq!(extension_pool::sync(&store, &manager).unwrap(), PoolSync::default());
    let runtime = store.load().unwrap().with_extension_providers_from(&manager);
    assert_eq!(runtime.providers.iter().filter(|provider| provider.relay.id == "imgenai").count(), 1);

    // 价格被改过（旧版本按公开价补的、或手动改的）：同步时改回声明的价格。
    store
        .update(|gateway| {
            let entry = gateway.providers.iter_mut().find(|entry| entry.source == source).unwrap();
            entry.model_prices.get_mut("gpt-6-astra").unwrap().input = 10.0;
            Ok(())
        })
        .unwrap();
    assert!(extension_pool::sync(&store, &manager).unwrap().changed);
    assert_eq!(pooled().pop().unwrap().model_prices["gpt-6-astra"].input, 17.5);
    assert_eq!(extension_pool::sync(&store, &manager).unwrap(), PoolSync::default());

    // 重新获取密钥：只换 Key，用户在池里改的名字保留。
    let renamed = ProviderInput {
        name: "我的 imgenai".into(),
        base_url: entry.relay.base_url.clone(),
        protocol: entry.relay.protocol,
        keys: Vec::new(),
        model: entry.relay.model.clone(),
        model_list: entry.relay.model_list.clone(),
        ..ProviderInput::default()
    };
    apply::update_provider(&store, "imgenai", &renamed).unwrap();
    set_key("sk-two");
    let second = extension_pool::sync(&store, &manager).unwrap();
    assert!(second.changed && second.created.is_empty());
    let entry = pooled().pop().unwrap();
    assert_eq!(key_of(&entry.relay), "sk-two");
    assert_eq!(entry.relay.name, "我的 imgenai");
    assert_eq!(entry.relay.id, "imgenai");

    // 退出登录（Key 清空）：移出池子。
    set_key("");
    assert!(extension_pool::sync(&store, &manager).unwrap().changed);
    assert!(pooled().is_empty());

    // 停用拓展也移出。
    set_key("sk-three");
    extension_pool::sync(&store, &manager).unwrap();
    assert_eq!(pooled().len(), 1);
    manager.set_script_enabled(IMGENAI_ACCOUNT_KEY, false).unwrap();
    assert!(extension_pool::sync(&store, &manager).unwrap().changed);
    assert!(pooled().is_empty());
    manager.set_script_enabled(IMGENAI_ACCOUNT_KEY, true).unwrap();

    // 用户自己手动加过同一地址的：不再重复写一份。
    let manual = ProviderInput {
        name: "手动 imgenai".into(),
        base_url: "https://api.imgenai.net/v1/".into(),
        keys: vec!["sk-manual".into()],
        model: "gpt-6.1-sol".into(),
        model_list: "gpt-6.1-sol".into(),
        ..ProviderInput::default()
    };
    apply::create_provider(&store, &manual, "custom", None).unwrap();
    assert!(!extension_pool::sync(&store, &manager).unwrap().changed);
    assert!(pooled().is_empty());
    assert_eq!(store.load().unwrap().providers.len(), 1);
}

#[test]
fn imgenai_account_scripts_only_use_public_interfaces() {
    let files = imgenai_account_files();
    for path in ["codexpp.json", "main.js", "manager.html", "backend/main.mjs"] {
        assert!(files.iter().any(|(name, _)| *name == path), "缺少 {path}");
    }
    for (path, content) in files {
        let source = String::from_utf8(content).unwrap();
        for private in ["__codexSessionDeleteBridge", "__CODEX_PLUS_", "postJson(", "codexPlusSettings("] {
            assert!(!source.contains(private), "{path} 还在用 {private}");
        }
        // 管理器面板在沙箱里，弹窗类接口用不了。
        if path == "manager.html" {
            for blocked in ["confirm(", "alert(", "prompt(", "window.open("] {
                assert!(!source.contains(blocked), "{path} 用了沙箱里不可用的 {blocked}");
            }
        }
    }
}
