# imgenai Code 分发版

imgenai Code 是在开源项目 [Codex++](https://github.com/BigPizzaV3/CodexPlusPlus)（AGPL-3.0）基础上修改的分发版。

## 和上游的差别放在哪

| 内容 | 位置 | 说明 |
| --- | --- | --- |
| imgenai 账户（登录、多账号切换、新建 / 删除 / 选用密钥、额度、重置卡、订阅与加油包） | `assets/extensions/imgenai-account/`，后台在 `crates/codex-plus-core/src/imgenai_account.rs` | 第六个官方内置拓展包，默认启用；后台内置在客户端里，不依赖 Node。本机可存多个账号（`session.json` 的 `accounts`），当前账号的密钥写进拓展配置 `apiKey`；每个账号记住自己选的密钥（`keyId`），没选过时用「桌面客户端」那把 |
| 客户端关着时 Codex 直连 imgenai、首次启动默认开机启动 | `apps/codex-plus-manager/src-tauri/src/imgenai.rs`、`crates/codex-plus-core/src/imgenai_codex_direct.rs` | 退出时把连着本机网关的 Codex 改成直连 api.imgenai.net，启动时最先改回；首次启动打开开机启动并隐藏到托盘（只设一次） |
| 开着代理时 Codex 连本机网关不走代理 | `crates/codex-plus-core/src/imgenai_loopback_proxy.rs`，调用点在 `imgenai.rs` | Windows 启动时给当前用户的 `NO_PROXY` 补 `localhost,127.0.0.1,::1`（setx）。Codex 的网络库不认系统代理例外里的 `127.*`，也不自动放过回环地址，大请求在代理那里失败回空 502 |
| 内置拓展包注册、默认启用 | `crates/codex-plus-core/src/bundled_extensions.rs`、`user_scripts.rs` | 几十行 |
| 拓展供应商 `requiresKey` | `user_scripts.rs` | 没有密钥时不并入网关 |
| 拓展供应商 `pool` | `crates/codex-plus-core/src/gateway/extension_pool.rs`，调用点在管理器的 `set_extension_config`、拓展开关与启动时 | 登录拿到密钥后把 imgenai 写进供应商池（看得到、能选模型），退出登录时移出 |
| 池里的 imgenai 自动补新模型 | `extension_pool.rs` 的 `offer_models`，上游检查在管理器 `gateway_commands.rs` 的 `watch_extension_pool_models` | 只加不删：新版 `codexpp.json` 多声明的模型在同步时补上；上游 `/models` 新出的每 6 小时查一次补上。给过的记在供应商的 `extensionOfferedModels`，用户删掉的不再加回。服务端上新型号后仍要在 `codexpp.json` 的 `prices` 补实扣单价并发版，否则用量页按公开价算 |
| 上手指引 | `apps/codex-plus-manager/src/ImgenaiGuide.tsx`（界面）、`imgenai-guide-model.ts`（按真实状态算当前步骤，有测试），接线在 `App.tsx` | 替代上游的入门清单与界面导览（上游那套教人「添加供应商」）：登录 → 开通 → 让 Codex 用 imgenai（可一键接入）→ 启动 Codex，每步看真实状态打勾，要点的元素用 `data-guide` / `data-agent-row` 等锚点圈出来。首次启动不再问使用模式，直接记成「只管理 Codex」。欢迎卡每次打开客户端都弹（已配好的显示怎么用），勾「以后不再弹出」才不自动弹（localStorage `imgenai.guide.noAutoOpen`），侧栏「上手指引」和「设置 → 通用 → 重新打开引导」随时再打开；面板里四步都能点开回看（做完的步骤讲在哪改），底部「上一步 / 下一步」翻页，配好的从第一步看起；侧栏进度卡只在没配好时出现 |
| 检查更新走国内下载站 | `crates/codex-plus-core/src/imgenai_update_mirror.rs`，调用点在 `update.rs` 的 `check_for_update` 与安装包下载 | 先读 `https://api.imgenai.net/download/latest.json`（同 GitHub 的 latest.json，安装包地址换成下载站），读不到再读 GitHub；安装包从下载站下不动时改从 GitHub 发布页下同一个文件。只在 `rebrand.mjs` 把更新源换成 imgenai-code 后生效。下载站由发版后的同步脚本更新，见下文「发版」 |
| 「添加供应商」目录的 imgenai 入口 | `apps/codex-plus-manager/src/GatewayProviders.tsx` | 点开跳到「imgenai 账户」页；构建时 `rebrand.mjs` 清掉上游赞助商（同行中转站） |
| 改名为 imgenai Code | `scripts/imgenai/rebrand.mjs` | **只在构建副本上执行**，源码分支保持上游写法 |
| 仓库首页 | `.github/README.md`、`README_EN.md`、`CONTRIBUTING.md`、`imgenai/hero.svg` | GitHub 优先显示 `.github/` 里的这几份；根目录同名文件保持上游原样，合并时不冲突 |
| 临时品牌图标 | `scripts/imgenai/brand/` | `make-icons.py` 生成，换正式图标时直接替换这几个文件 |

改名不提交进源码，是为了合并上游更新时不冲突：界面里约 2000 处「Codex++」，上游几乎每个版本都会改到其中一些。

## 打 macOS 安装包

```bash
bash scripts/imgenai/build-macos.sh
```

产物在 `~/Library/Caches/imgenai-code-build/release/src/dist/macos/`。构建目录特意放在仓库外：仓库在桌面上，桌面由 iCloud 同步，好几个 GB 的编译产物不能放进去，iCloud 加的扩展属性也会让 codesign 失败。本地开发跑测试同理，用 `CARGO_TARGET_DIR=~/Library/Caches/imgenai-code-build/dev-target`。

没有设置 `MACOS_SIGNING_IDENTITY` 时是 ad-hoc 签名，用户第一次打开要在「系统设置 → 隐私与安全性」里放行。

可选环境变量：`IMGENAI_UPDATE_REPO`（自动更新读取的 GitHub 仓库，默认 `gaoennan/imgenai-code`）、`IMGENAI_AD_LIST_URL`（首页推荐位，默认 `https://imgenai.net/desktop/ads.json`）。

## 发版

仓库：<https://github.com/gaoennan/imgenai-code>（公开，同时满足 AGPL 公开源码的要求）。上游自带的工作流在这个仓库里都已停用，只用 `.github/workflows/imgenai-release.yml`。

```bash
git tag imgenai-v1.0.1
git push origin imgenai-v1.0.1
```

推标签后云端构建 macOS 通用 DMG 与 Windows 安装包、生成 `latest.json`，**停在草稿**（只有仓库成员看得到）。确认安装包没问题后在 GitHub 网页上把草稿发布（勾选 Latest），客户端的「检查更新」才会读到这一版。

公开发布后，还要把这一版同步到国内下载站 `api.imgenai.net/download/`（同步脚本不在仓库里，由维护者在自己电脑上执行）。客户端检查更新先读下载站，下载站读得到就不会去读 GitHub，所以不同步的话，国内用户会一直停在上一版。

公证等超时（Apple 新账号头几次常排好几个小时）时 Mac 那一步会失败，但包和公证提交号已经存成这次运行的构建产物 `macos-dmg-pending`（留 14 天）。在这次运行上点「Re-run failed jobs」（或 `gh run rerun <运行号> --failed`），会接着等同一个提交，通过后装订、上传并补齐 `latest.json`，不重新打包，也不用从头排队。

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
