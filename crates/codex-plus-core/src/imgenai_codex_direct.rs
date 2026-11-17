//! imgenai 分发版：客户端退出后 Codex 照样能用。
//!
//! Codex 接入网关后，`config.toml` 指向客户端进程里的本机网关（127.0.0.1）。客户端一退出就没人监听，
//! Codex 一直重连。这里在客户端退出时把 Codex 改成直连 imgenai（api.imgenai.net），下次客户端启动时、
//! 在网关和其它启动逻辑之前原样改回来，网关自己的接入状态不用知道这件事。
//!
//! 只动两处：根键 `model_provider`，以及 `[model_providers.imgenai]` 这一段。原值存在状态目录的
//! `imgenai-codex-direct.json`，改回时只在这两处仍是这里写的样子时才还原；用户或别的工具在此期间
//! 改过的话，不去覆盖，交给网关现有的「被别的工具接管」处理。

use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::{Deserialize, Serialize};
use toml_edit::{DocumentMut, Item, Table};

/// 直连时 Codex 用的供应商 id，也是 `[model_providers.<id>]` 的段名。
pub const DIRECT_PROVIDER_ID: &str = "imgenai";
/// 与 imgenai 账户拓展清单里声明的供应商地址一致。
pub const DIRECT_BASE_URL: &str = "https://api.imgenai.net/v1";
const STASH_FILE: &str = "imgenai-codex-direct.json";

/// 切到直连前的原样：改回时用。
#[derive(Debug, Serialize, Deserialize)]
struct Stash {
    /// 根键 `model_provider` 原来的值；原来没有这个键时为 None。
    model_provider: Option<String>,
    /// 原来就有同名 `[model_providers.imgenai]` 时，那一段的原文（改回时整段放回）。
    provider_table: Option<String>,
}

fn stash_path(state_dir: &Path) -> PathBuf {
    state_dir.join(STASH_FILE)
}

fn config_path(codex_home: &Path) -> PathBuf {
    codex_home.join("config.toml")
}

/// 客户端退出时调用：Codex 现在连的是本机网关（`gateway_port`）时，改成直连 imgenai。
/// 返回是否改了。没登录 imgenai（`api_key` 为空）、Codex 没连网关、上次的改动还没改回时都不动。
pub fn switch_to_direct(codex_home: &Path, state_dir: &Path, gateway_port: u16, api_key: &str) -> anyhow::Result<bool> {
    let api_key = api_key.trim();
    if api_key.is_empty() || stash_path(state_dir).exists() {
        return Ok(false);
    }
    let path = config_path(codex_home);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Ok(false);
    };
    let mut doc: DocumentMut = text.parse().context("Codex 的 config.toml 解析失败")?;
    if !points_at_local_port(&doc, gateway_port) {
        return Ok(false);
    }
    let stash = Stash {
        model_provider: doc.get("model_provider").and_then(Item::as_str).map(str::to_string),
        provider_table: provider_table(&doc).map(table_text),
    };
    // 先存原样再改：写配置失败时原样还在，下次启动照常改回（改回时发现没改过就只删存档）。
    std::fs::create_dir_all(state_dir).with_context(|| format!("无法创建 {}", state_dir.display()))?;
    crate::settings::atomic_write(&stash_path(state_dir), serde_json::to_string_pretty(&stash)?.as_bytes())?;

    doc["model_provider"] = toml_edit::value(DIRECT_PROVIDER_ID);
    let mut table = Table::new();
    table["name"] = toml_edit::value("imgenai");
    table["base_url"] = toml_edit::value(DIRECT_BASE_URL);
    table["wire_api"] = toml_edit::value("responses");
    table["experimental_bearer_token"] = toml_edit::value(api_key);
    if !doc.contains_key("model_providers") {
        let mut providers = Table::new();
        providers.set_implicit(true);
        doc["model_providers"] = Item::Table(providers);
    }
    let providers = doc["model_providers"].as_table_mut().context("config.toml 里的 model_providers 不是表")?;
    providers.insert(DIRECT_PROVIDER_ID, Item::Table(table));
    crate::settings::atomic_write(&path, doc.to_string().as_bytes())?;
    Ok(true)
}

/// 客户端启动时、网关重新接入之前调用：退出时切过直连的，原样改回。返回是否改回了。
pub fn switch_back(codex_home: &Path, state_dir: &Path) -> anyhow::Result<bool> {
    let stash_file = stash_path(state_dir);
    let Ok(raw) = std::fs::read(&stash_file) else {
        return Ok(false);
    };
    let Ok(stash) = serde_json::from_slice::<Stash>(&raw) else {
        let _ = std::fs::remove_file(&stash_file);
        return Ok(false);
    };
    let path = config_path(codex_home);
    let mut doc: DocumentMut = match std::fs::read_to_string(&path) {
        Ok(text) => text.parse().context("Codex 的 config.toml 解析失败")?,
        Err(_) => {
            let _ = std::fs::remove_file(&stash_file);
            return Ok(false);
        }
    };
    let still_direct = doc.get("model_provider").and_then(Item::as_str) == Some(DIRECT_PROVIDER_ID);
    let ours = provider_table(&doc).is_some_and(|table| table.get("base_url").and_then(Item::as_str) == Some(DIRECT_BASE_URL));
    let mut changed = false;
    if still_direct && ours {
        match &stash.model_provider {
            Some(previous) => doc["model_provider"] = toml_edit::value(previous.as_str()),
            None => {
                doc.remove("model_provider");
            }
        }
        changed = true;
    }
    // 直连那段还是这里写的、且已经没人用它：放回原来的那段（原来没有就删掉，里面有密钥）。
    let still_used = doc.get("model_provider").and_then(Item::as_str) == Some(DIRECT_PROVIDER_ID);
    if ours && !still_used {
        let providers = doc["model_providers"].as_table_mut().context("config.toml 里的 model_providers 不是表")?;
        match stash.provider_table.as_deref().and_then(parse_table) {
            Some(previous) => {
                providers.insert(DIRECT_PROVIDER_ID, Item::Table(previous));
            }
            None => {
                providers.remove(DIRECT_PROVIDER_ID);
            }
        }
        if providers.is_empty() {
            doc.remove("model_providers");
        }
        changed = true;
    }
    if changed {
        crate::settings::atomic_write(&path, doc.to_string().as_bytes())?;
    }
    std::fs::remove_file(&stash_file).with_context(|| format!("无法删除 {}", stash_file.display()))?;
    Ok(changed && !still_used)
}

fn provider_table(doc: &DocumentMut) -> Option<&Table> {
    doc.get("model_providers")?.as_table()?.get(DIRECT_PROVIDER_ID)?.as_table()
}

/// 单独一段表的原文：放进一个只含它的文档里序列化，改回时再解析出来。
fn table_text(table: &Table) -> String {
    let mut doc = DocumentMut::new();
    doc.insert("t", Item::Table(table.clone()));
    doc.to_string()
}

fn parse_table(text: &str) -> Option<Table> {
    text.parse::<DocumentMut>().ok()?.get("t")?.as_table().cloned()
}

/// Codex 当前用的供应商地址是不是本机 `port`：官方供应商看根键 `openai_base_url`，其余看那一段的 `base_url`。
fn points_at_local_port(doc: &DocumentMut, port: u16) -> bool {
    let provider = doc.get("model_provider").and_then(Item::as_str).unwrap_or("openai");
    let endpoint = if provider == "openai" {
        doc.get("openai_base_url").and_then(Item::as_str)
    } else {
        doc.get("model_providers")
            .and_then(|providers| providers.get(provider))
            .and_then(|table| table.get("base_url"))
            .and_then(Item::as_str)
    };
    endpoint
        .and_then(|url| reqwest::Url::parse(url).ok())
        .is_some_and(|url| {
            matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "[::1]" | "::1")) && url.port_or_known_default() == Some(port)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    const GATEWAY_OFFICIAL: &str = r#"model = "gpt-6.1-sol"
model_provider = "openai"
openai_base_url = "http://127.0.0.1:57521/v1"

[projects."/tmp/x"]
trust_level = "trusted"
"#;

    const GATEWAY_CUSTOM: &str = r#"model = "gpt-6.1-sol"
model_provider = "custom"

[model_providers.custom]
name = "imgenai Code 网关"
wire_api = "responses"
base_url = "http://127.0.0.1:57521/gw/codex/v1"
experimental_bearer_token = "gw-token"
"#;

    fn setup(config: &str) -> (tempfile::TempDir, PathBuf, PathBuf) {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("codex");
        let state = temp.path().join("state");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(home.join("config.toml"), config).unwrap();
        (temp, home, state)
    }

    fn read(home: &Path) -> String {
        std::fs::read_to_string(home.join("config.toml")).unwrap()
    }

    #[test]
    fn round_trip_restores_the_gateway_config_exactly() {
        for original in [GATEWAY_OFFICIAL, GATEWAY_CUSTOM] {
            let (_temp, home, state) = setup(original);
            assert!(switch_to_direct(&home, &state, 57521, "sk-test").unwrap());
            let direct: DocumentMut = read(&home).parse().unwrap();
            assert_eq!(direct["model_provider"].as_str(), Some("imgenai"));
            assert_eq!(direct["model_providers"]["imgenai"]["base_url"].as_str(), Some(DIRECT_BASE_URL));
            assert_eq!(direct["model_providers"]["imgenai"]["experimental_bearer_token"].as_str(), Some("sk-test"));
            assert_eq!(direct["model"].as_str(), Some("gpt-6.1-sol"), "其它键不动");
            // 已经切过、还没改回时不再切第二次（否则存档会被直连状态覆盖）。
            assert!(!switch_to_direct(&home, &state, 57521, "sk-test").unwrap());

            assert!(switch_back(&home, &state).unwrap());
            assert_eq!(read(&home), original, "改回后与退出前一字不差");
            assert!(!stash_path(&state).exists());
            assert!(!switch_back(&home, &state).unwrap());
        }
    }

    #[test]
    fn leaves_codex_alone_when_not_on_this_gateway_or_logged_out() {
        let (_temp, home, state) = setup(GATEWAY_OFFICIAL);
        assert!(!switch_to_direct(&home, &state, 57521, "  ").unwrap(), "没登录 imgenai 不切");
        assert!(!switch_to_direct(&home, &state, 9999, "sk-test").unwrap(), "连的不是这个网关不切");
        let (_temp, home, state) = setup("model = \"gpt-5\"\n");
        assert!(!switch_to_direct(&home, &state, 57521, "sk-test").unwrap(), "官方直连不切");
        assert_eq!(read(&home), "model = \"gpt-5\"\n");
    }

    #[test]
    fn user_changes_while_closed_are_kept() {
        // 关着的时候用户把 Codex 改连别的供应商：不改回 model_provider，只删掉带密钥的直连段。
        let (_temp, home, state) = setup(GATEWAY_OFFICIAL);
        switch_to_direct(&home, &state, 57521, "sk-test").unwrap();
        let mut doc: DocumentMut = read(&home).parse().unwrap();
        doc["model_provider"] = toml_edit::value("other");
        std::fs::write(home.join("config.toml"), doc.to_string()).unwrap();
        switch_back(&home, &state).unwrap();
        let after: DocumentMut = read(&home).parse().unwrap();
        assert_eq!(after["model_provider"].as_str(), Some("other"));
        assert!(after.get("model_providers").and_then(|p| p.get("imgenai")).is_none(), "{after}");
        assert!(!stash_path(&state).exists());
    }

    #[test]
    fn existing_imgenai_section_is_put_back() {
        let original = format!("{GATEWAY_CUSTOM}\n[model_providers.imgenai]\nname = \"mine\"\nbase_url = \"https://example.com/v1\"\n");
        let (_temp, home, state) = setup(&original);
        assert!(switch_to_direct(&home, &state, 57521, "sk-test").unwrap());
        assert!(switch_back(&home, &state).unwrap());
        let after: DocumentMut = read(&home).parse().unwrap();
        assert_eq!(after["model_providers"]["imgenai"]["base_url"].as_str(), Some("https://example.com/v1"));
        assert_eq!(after["model_provider"].as_str(), Some("custom"));
    }
}
