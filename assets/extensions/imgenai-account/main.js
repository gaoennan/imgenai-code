// imgenai 账户在 Codex 里的入口：斜杠命令 /imgenai 查看今日额度。
// 登录、充值、重置卡都在管理器的「imgenai 账户」页面里，这里只读，不做写操作。
codexPlus.extension((ctx) => {
  const money = (value) => {
    const number = Number(value);
    return Number.isFinite(number) ? `$${number.toFixed(2)}` : "—";
  };

  ctx.registerCommand({
    name: "imgenai",
    description: "查看 imgenai 今日剩余额度",
    run: async () => {
      let result;
      try {
        result = await ctx.backend.call("overview");
      } catch (error) {
        ctx.toast(`读取 imgenai 账户失败：${error?.message || error}`);
        return;
      }
      if (!result?.ok) {
        ctx.toast(result?.message || "读取 imgenai 账户失败");
        return;
      }
      if (!result.loggedIn) {
        ctx.toast("还没登录 imgenai：打开 Codex++ 管理器的「imgenai 账户」页面登录");
        return;
      }
      const sub = result.sub || {};
      if (!sub.active) {
        ctx.toast("imgenai 订阅未开通或已到期：在「imgenai 账户」页面开通");
        return;
      }
      const cards = Number(sub.reset_cards) || 0;
      const pack = Number(sub.pack_remaining_usd) > 0 ? `，加油包 ${money(sub.pack_remaining_usd)}` : "";
      ctx.toast(`imgenai ${sub.tier_label || ""}：今日剩余 ${money(sub.daily_remaining_usd)} / ${money(sub.daily_cap_usd)}${pack}${cards ? `，重置卡 ${cards} 张` : ""}`);
    },
  });
});
