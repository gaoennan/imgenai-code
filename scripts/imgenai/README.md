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
