//! 拓展声明了 `"pool": true` 的供应商：拿到 Key 后写进用户的供应商池（gateway.json）。
//!
//! 默认的拓展供应商只在构建 Agent 模型资料与网关路由时并入，管理器的供应商池里看不到，
//! 接入 Agent 时也选不到它的模型。账户类拓展（登录后才有 Key，比如 imgenai 账户）要的是
//! 「登录了就出现在供应商池里」，所以写成真正的池成员：
//! - 第一次拿到 Key：按声明新建，来源记为 `extension-pool:<拓展 id>/<供应商 id>`；
//!   用户已经手动加过同一地址的供应商时不重复建。
//! - Key 变了：只换 Key，用户在池里改过的名字、模型等都保留。
//! - 声明了 `prices` 的模型：价格以声明为准（新建时写入，之后每次同步发现不同就改回），
//!   用量页按它算的费用与服务商实际扣费一致；没声明的模型照旧按公开价格表补。
//! - Key 清空（退出登录）、拓展停用或删除：从池里移除（连同 Agent 里指向它的模型，同手动删除）。
//! - 新模型自动补进列表（只加不删）：新版拓展声明的、以及声明了 `discoverModels` 的上游新出的
//!   （见 [`offer_models`]）。给过的记在供应商上，用户自己删掉的不会再被加回来。
//! - 声明了 `fast` 的：池里这家的每个模型（包括之后补进来的）都在 `model_metadata` 里标成支持 Fast，
//!   Codex 的模型目录带上 Fast 档位，界面才有 Fast 按钮。用户自己给某个模型写过 Fast 声明的不改。
//!
//! 池里有了同 id 的成员，路由时拓展自己的那份就不再并入（以池里的为准），不会出现两份。

use crate::gateway::apply::{self, ProviderInput};
use crate::gateway::model_sync::{append_models, has_model};
use crate::gateway::store::{GatewayProvider, GatewayStore};
use crate::settings::{RelayProfile, RelayProtocol};
use crate::user_scripts::{ExtensionProvider, UserScriptManager};

/// 拓展写进池里的供应商的来源前缀。
pub const SOURCE_PREFIX: &str = "extension-pool:";

/// 拓展写进池里的供应商的来源：`extension-pool:<拓展 id>/<供应商 id>`。
pub fn pool_source(extension: &str, provider: &str) -> String {
    format!("{SOURCE_PREFIX}{extension}/{provider}")
}

/// 这次同步做了什么。`created` 是新建的供应商 id 里声明了 `discoverModels` 的，调用方随后从上游拉模型列表。
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PoolSync {
    pub changed: bool,
    pub created: Vec<String>,
}

pub fn sync(store: &GatewayStore, manager: &UserScriptManager) -> anyhow::Result<PoolSync> {
    let wanted: Vec<(String, ExtensionProvider)> = manager
        .enabled_extension_providers()?
        .into_iter()
        .filter(|(_, provider)| provider.pool && !provider.api_key.trim().is_empty())
        .map(|(extension, provider)| (pool_source(&extension, &provider.id), provider))
        .collect();
    let mut outcome = PoolSync::default();

    let stale: Vec<String> = store
        .load()?
        .providers
        .iter()
        .filter(|entry| entry.source.starts_with(SOURCE_PREFIX) && !wanted.iter().any(|(source, _)| *source == entry.source))
        .map(|entry| entry.relay.id.clone())
        .collect();
    for id in stale {
        apply::delete_provider(store, &id)?;
        outcome.changed = true;
    }

    for (source, provider) in &wanted {
        let gateway = store.load()?;
        let key = provider.api_key.trim();
        if let Some(existing) = gateway.providers.iter().find(|entry| entry.source == *source) {
            let id = existing.relay.id.clone();
            if current_key(&existing.relay) != key {
                let relay = &existing.relay;
                let input = ProviderInput {
                    name: relay.name.clone(),
                    base_url: relay.base_url.clone(),
                    protocol: relay.protocol,
                    keys: vec![key.to_string()],
                    model: relay.model.clone(),
                    model_list: relay.model_list.clone(),
                    ..ProviderInput::default()
                };
                apply::update_provider(store, &id, &input)?;
                outcome.changed = true;
            }
            // 新版拓展多声明的模型补进列表（旧版建的成员列表里没有）。
            if offer_models(store, &id, &provider.models)?.changed {
                outcome.changed = true;
            }
            // 声明了价格的以声明为准：服务商调价后跟着新版拓展更新，用量页的费用与服务商扣费保持一致。
            let stale_prices = provider
                .prices
                .iter()
                .any(|(model, price)| existing.model_prices.get(model) != Some(price));
            if mark_fast(store, &id, provider.fast)? {
                outcome.changed = true;
            }
            if stale_prices {
                store.update(|gateway| {
                    if let Some(entry) = gateway.providers.iter_mut().find(|entry| entry.relay.id == id) {
                        for (model, price) in &provider.prices {
                            entry.model_prices.insert(model.clone(), price.clone());
                        }
                    }
                    Ok(())
                })?;
                outcome.changed = true;
            }
            continue;
        }
        if gateway.providers.iter().any(|entry| same_url(&entry.relay.base_url, &provider.base_url)) {
            continue;
        }
        let input = ProviderInput {
            name: provider.name.clone(),
            base_url: provider.base_url.clone(),
            protocol: protocol(&provider.protocol),
            keys: vec![key.to_string()],
            model: provider.models.first().cloned().unwrap_or_default(),
            model_list: provider.models.join("\n"),
            model_prices: (!provider.prices.is_empty()).then(|| provider.prices.clone()),
            ..ProviderInput::default()
        };
        let (_, id) = apply::create_provider(store, &input, source, None)?;
        // 声明的模型记为给过，之后只补新出的。
        offer_models(store, &id, &provider.models)?;
        mark_fast(store, &id, provider.fast)?;
        outcome.changed = true;
        if provider.discover_models {
            outcome.created.push(id);
        }
    }
    Ok(outcome)
}

/// 与 `GatewaySettings::merge_extension_providers` 的协议映射一致。
fn protocol(name: &str) -> RelayProtocol {
    match name {
        "chat" => RelayProtocol::ChatCompletions,
        "anthropic" => RelayProtocol::AnthropicMessages,
        _ => RelayProtocol::Responses,
    }
}

fn current_key(relay: &RelayProfile) -> &str {
    relay
        .api_keys
        .first()
        .map(|entry| entry.api_key.as_str())
        .unwrap_or(relay.api_key.as_str())
        .trim()
}

/// 拓展已经给过这个成员的模型，存在供应商的 `extra` 里（不占正式字段，旧版本读写时原样保留）。
const OFFERED_FIELD: &str = "extensionOfferedModels";

/// [`offer_models`] 做了什么。
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Offered {
    /// 实际加进模型列表的。
    pub added: Vec<String>,
    /// 写过 gateway.json（加了模型，或只是记下了给过的模型）。
    pub changed: bool,
}

/// 把 `models` 里没给过的模型追加到拓展池成员 `id` 的列表末尾（只加不删、不动顺序和默认模型），
/// 并记为给过。用户从列表里删掉的模型已经给过，以后不会再被加回来。
/// 第一次调用（升级前建的成员还没有记录）时，把当时列表里的模型当作给过的。
/// 不是拓展池成员（手动加的供应商等）不动。
pub fn offer_models(store: &GatewayStore, id: &str, models: &[String]) -> anyhow::Result<Offered> {
    let pooled = |provider: &&mut GatewayProvider| provider.relay.id == id && provider.source.starts_with(SOURCE_PREFIX);
    // 先在副本上看一遍：没有要写的就不碰文件（启动、改拓展配置时都会同步一次）。
    let mut gateway = store.load()?;
    let Some(provider) = gateway.providers.iter_mut().find(pooled) else {
        return Ok(Offered::default());
    };
    if !offer_into(provider, models)?.changed {
        return Ok(Offered::default());
    }
    let (_, offered) = store.update(|gateway| match gateway.providers.iter_mut().find(pooled) {
        Some(provider) => offer_into(provider, models),
        None => Ok(Offered::default()),
    })?;
    Ok(offered)
}

fn offer_into(provider: &mut GatewayProvider, models: &[String]) -> anyhow::Result<Offered> {
    let current = crate::gateway::route::relay_models(&provider.relay);
    let recorded = provider
        .extra
        .get(OFFERED_FIELD)
        .and_then(|value| serde_json::from_value::<Vec<String>>(value.clone()).ok());
    let first_time = recorded.is_none();
    let mut offered = recorded.unwrap_or_else(|| current.clone());
    let mut fresh: Vec<String> = Vec::new();
    for model in models.iter().map(|model| model.trim()).filter(|model| !model.is_empty()) {
        if !has_model(&offered, model) && !has_model(&fresh, model) {
            fresh.push(model.to_string());
        }
    }
    if !first_time && fresh.is_empty() {
        return Ok(Offered::default());
    }
    let missing: Vec<String> = fresh.iter().filter(|model| !has_model(&current, model)).cloned().collect();
    let added = if missing.is_empty() { Vec::new() } else { append_models(provider, &missing)? };
    offered.extend(fresh);
    provider.extra.insert(OFFERED_FIELD.to_string(), serde_json::json!(offered));
    // 新补进来的模型也标上 Fast（拓展声明了 `fast` 时）。
    declare_fast_models(provider);
    Ok(Offered { added, changed: true })
}

/// 拓展声明了 `fast`，记在供应商的 `extra` 里，之后从上游补进来的模型也据此标上 Fast。
const FAST_FIELD: &str = "extensionFastSupport";
/// 模型元数据里的 Fast 声明，见 [`crate::model_suffix::apply_model_fast_support`]。
const FAST_DECLARATION: &str = "codex_plus_fast_support";

/// 按拓展的 `fast` 声明更新池里这家：记下声明，并给现有模型都标上 Fast。没有要写的就不碰文件。
fn mark_fast(store: &GatewayStore, id: &str, fast: bool) -> anyhow::Result<bool> {
    let pooled = |provider: &&mut GatewayProvider| provider.relay.id == id && provider.source.starts_with(SOURCE_PREFIX);
    let mut gateway = store.load()?;
    let Some(provider) = gateway.providers.iter_mut().find(pooled) else {
        return Ok(false);
    };
    if !fast_into(provider, fast) {
        return Ok(false);
    }
    let (_, changed) = store.update(|gateway| {
        Ok(gateway.providers.iter_mut().find(pooled).is_some_and(|provider| fast_into(provider, fast)))
    })?;
    Ok(changed)
}

fn fast_into(provider: &mut GatewayProvider, fast: bool) -> bool {
    let recorded = provider.extra.get(FAST_FIELD).and_then(serde_json::Value::as_bool) == Some(true);
    let mut changed = false;
    if fast && !recorded {
        provider.extra.insert(FAST_FIELD.to_string(), serde_json::Value::Bool(true));
        changed = true;
    } else if !fast && recorded {
        // 新版拓展不再声明：之后补的模型不再标，已经标上的保留（用户可能在用）。
        provider.extra.remove(FAST_FIELD);
        changed = true;
    }
    declare_fast_models(provider) || changed
}

/// 给池里这家没写过 Fast 声明的模型标上「支持」。`model_metadata` 不是合法 JSON 对象时不动（留给用户改）。
fn declare_fast_models(provider: &mut GatewayProvider) -> bool {
    if provider.extra.get(FAST_FIELD).and_then(serde_json::Value::as_bool) != Some(true) {
        return false;
    }
    let raw = provider.relay.model_metadata.trim();
    let mut metadata = if raw.is_empty() {
        serde_json::Map::new()
    } else {
        match serde_json::from_str::<serde_json::Value>(raw) {
            Ok(serde_json::Value::Object(map)) => map,
            _ => return false,
        }
    };
    let mut changed = false;
    for model in crate::gateway::route::relay_models(&provider.relay) {
        let existing = metadata.keys().find(|key| key.eq_ignore_ascii_case(&model)).cloned();
        match existing.and_then(|key| metadata.get_mut(&key)) {
            Some(serde_json::Value::Object(entry)) => {
                if !entry.contains_key(FAST_DECLARATION) {
                    entry.insert(FAST_DECLARATION.to_string(), serde_json::json!("supported"));
                    changed = true;
                }
            }
            // 不是对象的条目解析时会报错，不替用户改。
            Some(_) => {}
            None => {
                metadata.insert(model, serde_json::json!({ FAST_DECLARATION: "supported" }));
                changed = true;
            }
        }
    }
    if changed {
        provider.relay.model_metadata = serde_json::Value::Object(metadata).to_string();
    }
    changed
}

/// 声明了 `discoverModels` 的拓展池成员的来源：这些成员上游一出新模型就自动补进列表
/// （调用方定期拉上游，交给 [`offer_models`]）。
pub fn discovering_sources(manager: &UserScriptManager) -> Vec<String> {
    manager
        .enabled_extension_providers()
        .unwrap_or_default()
        .into_iter()
        .filter(|(_, provider)| provider.pool && provider.discover_models)
        .map(|(extension, provider)| pool_source(&extension, &provider.id))
        .collect()
}

fn same_url(left: &str, right: &str) -> bool {
    let clean = |url: &str| url.trim().trim_end_matches('/').to_ascii_lowercase();
    clean(left) == clean(right)
}

#[cfg(test)]
mod tests {
    use crate::settings::{RelayMode, RelayProfile};

    /// 内置资料里还没有的新模型（如 gpt-6.2-sol）默认不带 Fast 档位；标上声明后目录里才有，Codex 才显示 Fast 按钮。
    #[test]
    fn fast_declaration_adds_fast_tier_to_member_catalog() {
        let relay = |metadata: &str| RelayProfile {
            id: "imgenai".into(),
            name: "imgenai".into(),
            model: "gpt-6.2-sol".into(),
            model_list: "gpt-6.2-sol".into(),
            base_url: "https://api.imgenai.net/v1".into(),
            api_key: "sk-test".into(),
            relay_mode: RelayMode::PureApi,
            model_metadata: metadata.into(),
            ..RelayProfile::default()
        };
        let tiers = |profile: &RelayProfile| -> Vec<String> {
            let payload = crate::aggregate_catalog::build_member_catalog(profile).unwrap();
            let entry = payload["models"]
                .as_array()
                .unwrap()
                .iter()
                .find(|entry| entry["slug"] == "gpt-6.2-sol")
                .cloned()
                .unwrap();
            entry["service_tiers"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|tier| tier.get("id").or(Some(tier)).and_then(serde_json::Value::as_str).map(str::to_string))
                .collect()
        };
        assert!(!tiers(&relay("")).iter().any(|tier| tier == "priority"));
        let declared = r#"{"gpt-6.2-sol":{"codex_plus_fast_support":"supported"}}"#;
        assert!(tiers(&relay(declared)).iter().any(|tier| tier == "priority"));
    }
}
