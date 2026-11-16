# imgenai Code 分发版

imgenai Code 是在开源项目 [Codex++](https://github.com/BigPizzaV3/CodexPlusPlus)（AGPL-3.0）基础上修改的分发版。

## 和上游的差别放在哪

| 内容 | 位置 | 说明 |
| --- | --- | --- |
| imgenai 账户（登录、额度、重置卡、订阅与加油包） | `assets/extensions/imgenai-account/` | 第六个官方内置拓展包，默认启用 |
| 内置拓展包注册、默认启用 | `crates/codex-plus-core/src/bundled_extensions.rs`、`user_scripts.rs` | 几十行 |
| 拓展供应商 `requiresKey` | `user_scripts.rs` | 没有密钥时不并入网关 |
| 改名为 imgenai Code | `scripts/imgenai/rebrand.mjs` | **只在构建副本上执行**，源码分支保持上游写法 |
| 临时品牌图标 | `scripts/imgenai/brand/` | `make-icons.py` 生成，换正式图标时直接替换这几个文件 |

改名不提交进源码，是为了合并上游更新时不冲突：界面里约 2000 处「Codex++」，上游几乎每个版本都会改到其中一些。

## 打 macOS 安装包

```bash
bash scripts/imgenai/build-macos.sh
```

产物在 `~/Library/Caches/imgenai-code-build/release/src/dist/macos/`。构建目录特意放在仓库外：仓库在桌面上，桌面由 iCloud 同步，好几个 GB 的编译产物不能放进去，iCloud 加的扩展属性也会让 codesign 失败。本地开发跑测试同理，用 `CARGO_TARGET_DIR=~/Library/Caches/imgenai-code-build/dev-target`。

没有设置 `MACOS_SIGNING_IDENTITY` 时是 ad-hoc 签名，用户第一次打开要在「系统设置 → 隐私与安全性」里放行。

可选环境变量：`IMGENAI_UPDATE_REPO`（自动更新读取的 GitHub 仓库，默认 `imgenai/imgenai-code`）、`IMGENAI_AD_LIST_URL`（首页推荐位，默认 `https://imgenai.net/desktop/ads.json`）。

## 发版

仓库：<https://github.com/gaoennan/imgenai-code>（公开，同时满足 AGPL 公开源码的要求）。上游自带的工作流在这个仓库里都已停用，只用 `.github/workflows/imgenai-release.yml`。

```bash
git tag imgenai-v1.0.1
git push origin imgenai-v1.0.1
```

推标签后云端构建 macOS 通用 DMG 与 Windows 安装包、生成 `latest.json`，**停在草稿**（只有仓库成员看得到）。确认安装包没问题后在 GitHub 网页上把草稿发布（勾选 Latest），客户端的「检查更新」才会读到这一版。

### macOS 正式签名与公证

仓库 Secrets 里配了下面几项时，macOS 包会用 Developer ID 正式签名并提交 Apple 公证；没配时 ad-hoc 签名（用户首次打开要在「系统设置 → 隐私与安全性」里放行）。

已配置（2026-11-16）：证书「Developer ID Application: Ennan Gao (3W7Z852F27)」，2027-12-16 到期；公证用 App Store Connect API 密钥「imgenai Code notarization」（权限：开发者）。私钥、证书与 .p8 在本机 `~/Library/Application Support/imgenai-code-signing/`（仅本人可读，别放进 iCloud 同步的目录）。证书到期或要换机器时，用 `scripts/imgenai/upload-signing-secrets.sh` 重新上传。

| Secret | 内容 |
| --- | --- |
| `MACOS_CERT_P12` | 「Developer ID Application」证书导出的 .p12，base64 编码 |
| `MACOS_CERT_PASSWORD` | 导出 .p12 时设的密码 |
| `MACOS_SIGNING_IDENTITY` | 证书全名，如 `Developer ID Application: 名字 (团队ID)` |
| `APPLE_API_KEY_P8` | App Store Connect API 密钥 .p8，base64 编码 |
| `APPLE_API_KEY_ID` | 该密钥的 Key ID |
| `APPLE_API_ISSUER_ID` | Issuer ID |

版本号是 imgenai Code 自己的（从 1.0.0 起），由 `prepare-src.mjs --version` 在构建副本里写进所有带版本号的文件，源码里仍是上游的版本号。客户端按数字逐段比较版本号，新版本号必须比已发布的大。

## 合并上游更新

```bash
git fetch upstream
git merge upstream/main
```

然后打一次包。`rebrand.mjs` 里每条「只对个别文件」的规则都必须命中，上游改了那几处写法时脚本会直接报错并列出没命中的规则，照着新写法更新规则即可。

## 测试

**Rust 测试一律用 `bash scripts/imgenai/test.sh`，不要直接 `cargo test`。** 上游有测试会写 `$HOME` 下的真实配置：`gateway_e2e` 会把 `~/.claude/settings.json` 改成指向本地测试网关（本机 Claude Code 随即连不上），其它测试会在 `~/.config/`、`~/` 下建状态目录。脚本把 `HOME` 指到临时目录，并去掉代理环境变量（否则本地网络测试会假失败）。前端测试 `npm test` 不受影响。

`update::macos::tests::macos_update_detached_signed_helper_executes_without_modifying_source_app` 在没有开发者签名的机器上上游原版也会失败；`connect::tests::unanswered_polls_trigger_health_retry_and_recover` 在上游原版上也偶发失败。

## 许可

改动后的完整源码按 AGPL-3.0 向用户公开；保留上游 `LICENSE`、`THIRD_PARTY_NOTICES.md` 与版权声明。应用「关于」页注明基于 Codex++ 修改并链接原项目。
