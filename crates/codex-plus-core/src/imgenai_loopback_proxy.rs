//! imgenai 分发版：开着代理时，Codex 发往本机网关（127.0.0.1）的请求不能被交给代理。
//!
//! Codex 内置的网络库（reqwest 0.12 / hyper-util 的代理匹配）有两处不放过回环地址：
//! - 环境变量 `HTTP_PROXY` / `ALL_PROXY` 设了代理时，127.0.0.1 也照样走代理；
//! - 环境变量里没有 `NO_PROXY` 时读 Windows 系统代理的例外列表（`ProxyOverride`），但 Clash、v2rayN
//!   默认写的 `127.*`、`192.168.*` 这类带星号的条目它不认，127.0.0.1 还是走代理。
//!
//! 请求小的时候代理转得过去，对话里图片一多、请求到了几 MB，就在代理那里失败，代理回一个空的 502，
//! Codex 显示「unexpected status 502 Bad Gateway: Unknown error, url: http://127.0.0.1:…」。
//! 有 `NO_PROXY` 时网络库以它为准（系统代理地址照用），所以给当前用户的环境变量 `NO_PROXY` 补上回环地址。

/// 补进 `NO_PROXY` 的回环地址。
pub const LOOPBACK_ENTRIES: [&str; 3] = ["localhost", "127.0.0.1", "::1"];

/// 在已有的 `NO_PROXY` 后面补上缺的回环地址（保留原有条目和顺序，不分大小写去重）；都已经有了时返回 None。
pub fn merged_no_proxy(existing: Option<&str>) -> Option<String> {
    let mut entries: Vec<String> = existing
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(str::to_string)
        .collect();
    let before = entries.len();
    for wanted in LOOPBACK_ENTRIES {
        if !entries.iter().any(|entry| entry.eq_ignore_ascii_case(wanted)) {
            entries.push(wanted.to_string());
        }
    }
    (entries.len() != before).then(|| entries.join(","))
}

/// `setx` 只收 1024 个字符以内的值，再长会被截断、把用户原有的设置弄坏，这种情况不写。
#[cfg(windows)]
const SETX_MAX_CHARS: usize = 1000;

/// Windows：当前用户的环境变量 `NO_PROXY` 缺回环地址时补上。返回是否写了。
///
/// 用户级没有时沿用系统级的值再补（用户级同名变量会盖住系统级，不能丢掉系统级原有的条目）。
/// 用 `setx` 写：它写进 `HKCU\Environment` 并广播环境变量变更，之后新启动的程序（包括 Codex）就能读到；
/// 已经在运行的 Codex 要重启一次才生效。
#[cfg(windows)]
pub fn ensure_windows_user_no_proxy() -> anyhow::Result<bool> {
    use std::os::windows::process::CommandExt;
    let find = |pairs: Vec<(String, Option<String>)>| {
        pairs
            .into_iter()
            .find(|(name, _)| name.eq_ignore_ascii_case("NO_PROXY"))
            .and_then(|(_, value)| value)
    };
    let user = find(crate::windows_integration::read_current_user_string_values("Environment")?);
    let existing = user.or_else(|| {
        find(
            crate::windows_integration::read_local_machine_string_values(
                r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment",
            )
            .unwrap_or_default(),
        )
    });
    let Some(merged) = merged_no_proxy(existing.as_deref()) else {
        return Ok(false);
    };
    anyhow::ensure!(
        merged.chars().count() <= SETX_MAX_CHARS,
        "NO_PROXY 太长（{} 个字符），setx 会截断，没有改动",
        merged.chars().count()
    );
    let status = std::process::Command::new("setx")
        .args(["NO_PROXY", merged.as_str()])
        .creation_flags(crate::windows_create_no_window())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()?;
    anyhow::ensure!(status.success(), "setx 写入 NO_PROXY 失败（{status}）");
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds_loopback_when_missing() {
        assert_eq!(merged_no_proxy(None).as_deref(), Some("localhost,127.0.0.1,::1"));
        assert_eq!(merged_no_proxy(Some(" , ")).as_deref(), Some("localhost,127.0.0.1,::1"));
    }

    #[test]
    fn keeps_existing_entries_and_order() {
        assert_eq!(
            merged_no_proxy(Some("corp.example.com, 10.0.0.0/8,LOCALHOST")).as_deref(),
            Some("corp.example.com,10.0.0.0/8,LOCALHOST,127.0.0.1,::1")
        );
    }

    #[test]
    fn leaves_complete_value_alone() {
        assert_eq!(merged_no_proxy(Some("localhost,127.0.0.1,::1,intranet")), None);
    }
}
