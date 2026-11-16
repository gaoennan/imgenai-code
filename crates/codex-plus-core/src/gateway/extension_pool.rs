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
//!
//! 池里有了同 id 的成员，路由时拓展自己的那份就不再并入（以池里的为准），不会出现两份。

use crate::gateway::apply::{self, ProviderInput};
use crate::gateway::store::GatewayStore;
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
            // 声明了价格的以声明为准：服务商调价后跟着新版拓展更新，用量页的费用与服务商扣费保持一致。
            let stale_prices = provider
                .prices
                .iter()
                .any(|(model, price)| existing.model_prices.get(model) != Some(price));
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

fn same_url(left: &str, right: &str) -> bool {
    let clean = |url: &str| url.trim().trim_end_matches('/').to_ascii_lowercase();
    clean(left) == clean(right)
}
