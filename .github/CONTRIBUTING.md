# 参与 imgenai Code

感谢你帮忙改进 imgenai Code。

## 反馈问题

- 订阅、扣费、账户相关的问题请到 [imgenai 官网](https://imgenai.net) 联系客服，处理更快。
- 客户端的问题在本仓库提 Issue，写清系统、imgenai Code 版本、复现步骤和实际看到的报错。
- 贴日志或截图前请隐去中转密钥、登录令牌和手机号。

## 提交改动

- imgenai 专有的部分集中在 `scripts/imgenai/` 和 `assets/extensions/imgenai-account/`，改之前先读 [scripts/imgenai/README.md](../scripts/imgenai/README.md)。
- 网关、会话管理、Codex 增强等通用功能来自上游 [Codex++](https://github.com/BigPizzaV3/CodexPlusPlus)，这类修复欢迎直接贡献给上游，下次合并时会一起带过来。
- Rust 测试请用 `bash scripts/imgenai/test.sh`，不要直接 `cargo test`：上游有测试会改写本机 `~/.claude` 等真实配置。

提交的代码按本仓库的 [AGPL-3.0](../LICENSE) 许可发布。
