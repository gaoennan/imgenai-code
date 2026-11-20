//! imgenai：检查更新和下载安装包先走国内下载站 api.imgenai.net/download/，读不到、下不动再回落 GitHub。
//!
//! 国内直连 GitHub 的发布附件经常很慢甚至下不动。下载站的 `latest.json` 和 GitHub 发布页上的是同一份，
//! 只是安装包地址换成下载站（发版时由同步脚本生成）。
//!
//! 只对 imgenai Code 的构建生效：`rebrand.mjs` 把更新源换成 imgenai-code 仓库后才启用；
//! 源码分支保持上游地址，跑测试和上游合并都不受影响。

/// 下载站上的更新清单。
pub const MIRROR_LATEST_JSON_URL: &str = "https://api.imgenai.net/download/latest.json";
/// 下载站清单里安装包地址的前缀，后面直接跟安装包文件名。
const MIRROR_ASSET_PREFIX: &str = "https://api.imgenai.net/download/imgenai-code/";
/// imgenai Code 的发版标签写法：imgenai-v1.0.10。
const RELEASE_TAG_PREFIX: &str = "imgenai-v";

fn is_imgenai_update_source(default_latest_json_url: &str) -> bool {
    default_latest_json_url.contains("/imgenai-code/releases/")
}

/// 读更新清单的顺序：imgenai Code 先读下载站、再读 GitHub；其余构建只读原来的地址。
pub fn latest_json_urls(default_latest_json_url: &str) -> Vec<String> {
    if is_imgenai_update_source(default_latest_json_url) {
        vec![MIRROR_LATEST_JSON_URL.to_string(), default_latest_json_url.to_string()]
    } else {
        vec![default_latest_json_url.to_string()]
    }
}

/// 安装包从下载站下不动时，同一个安装包在 GitHub 发布页上的地址。
/// 下载站地址 `…/download/imgenai-code/<文件名>` 对应 GitHub 的 `releases/download/imgenai-v<版本>/<文件名>`。
/// 不是下载站地址（比如本来就是 GitHub 地址）时返回 `None`，不重复下载。
pub fn github_fallback_asset_url(default_latest_json_url: &str, version: &str, asset_url: &str) -> Option<String> {
    if !is_imgenai_update_source(default_latest_json_url) {
        return None;
    }
    let name = asset_url.strip_prefix(MIRROR_ASSET_PREFIX)?;
    if name.is_empty() || name.contains(['/', '?', '#']) {
        return None;
    }
    let releases = default_latest_json_url.strip_suffix("latest/download/latest.json")?;
    let version = version.trim().trim_start_matches(['v', 'V']);
    if version.is_empty() {
        return None;
    }
    Some(format!("{releases}download/{RELEASE_TAG_PREFIX}{version}/{name}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const IMGENAI: &str = "https://github.com/gaoennan/imgenai-code/releases/latest/download/latest.json";
    const UPSTREAM: &str = "https://github.com/BigPizzaV3/CodexPlusPlus/releases/latest/download/latest.json";

    #[test]
    fn imgenai_build_reads_mirror_first_then_github() {
        assert_eq!(latest_json_urls(IMGENAI), vec![MIRROR_LATEST_JSON_URL.to_string(), IMGENAI.to_string()]);
        assert_eq!(latest_json_urls(UPSTREAM), vec![UPSTREAM.to_string()]);
    }

    #[test]
    fn mirror_asset_maps_to_same_file_on_github_release() {
        assert_eq!(
            github_fallback_asset_url(
                IMGENAI,
                "v1.0.10",
                "https://api.imgenai.net/download/imgenai-code/imgenaiCode-1.0.10-macos-universal.dmg",
            )
            .as_deref(),
            Some("https://github.com/gaoennan/imgenai-code/releases/download/imgenai-v1.0.10/imgenaiCode-1.0.10-macos-universal.dmg"),
        );
        assert_eq!(
            github_fallback_asset_url(IMGENAI, "1.0.10", "https://api.imgenai.net/download/imgenai-code/imgenaiCode-1.0.10-windows-x64-setup.exe")
                .as_deref(),
            Some("https://github.com/gaoennan/imgenai-code/releases/download/imgenai-v1.0.10/imgenaiCode-1.0.10-windows-x64-setup.exe"),
        );
    }

    #[test]
    fn no_fallback_for_github_urls_other_builds_or_odd_names() {
        let github = "https://github.com/gaoennan/imgenai-code/releases/download/imgenai-v1.0.10/imgenaiCode-1.0.10-macos-universal.dmg";
        assert_eq!(github_fallback_asset_url(IMGENAI, "v1.0.10", github), None);
        assert_eq!(
            github_fallback_asset_url(UPSTREAM, "v1.0.10", "https://api.imgenai.net/download/imgenai-code/a.dmg"),
            None
        );
        for odd in ["", "x/a.dmg", "a.dmg?x=1"] {
            let url = format!("https://api.imgenai.net/download/imgenai-code/{odd}");
            assert_eq!(github_fallback_asset_url(IMGENAI, "v1.0.10", &url), None, "{url}");
        }
        assert_eq!(github_fallback_asset_url(IMGENAI, "", "https://api.imgenai.net/download/imgenai-code/a.dmg"), None);
    }
}
