// imgenai 账户的后台已经内置在客户端里（crates/codex-plus-core/src/imgenai_account.rs），不再用 Node 运行：
// Windows 上多数人没有 Node，原来的 Node 后台在那里起不来，连登录都做不了。
//
// 这个文件只为让清单里的 backend.script 有个落脚处：页面要有 backend 声明才拿得到 ctx.backend，
// 而内置的 imgenai-account 拓展的后台调用在宿主里直接转给客户端，不会执行这里。
export default () => ({});
