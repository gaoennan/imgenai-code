  /**
   * 内置拓展「显示 Max / Ultra 档位」。
   *
   * Codex 模型选择器里的档位有两道门：
   * - Max：本地设置 `[desktop] enabled-reasoning-efforts` 默认不含 max。这一项由 Rust 侧在启动 Codex 前
   *   补进 config.toml（relay_config::ensure_max_ultra_reasoning_efforts_in_home），页面侧不用管。
   * - Ultra：要远程开关 multiAgentMode（闸门 1186680773）为开。这份数据由 OpenAI 后端下发，国内网络下拉不到时
   *   Statsig 客户端 Ready 但没有任何数据，所有远程开关按关处理，Ultra 随之消失。
   *
   * 做法与「替我审批」（33-offline-auto-review.js）相同：只在客户端完全没有远程数据时补一份最小数据，
   * 只多开 multiAgentMode 这一个闸门；拿到了真实数据（开了代理）就完全不碰。只写内存，关掉开关后重启 Codex 即恢复。
   */
  const codexPlusMultiAgentModeGate = "1186680773";

  function codexPlusReasoningEffortUnlockEnabled() {
    return codexPlusBackendSettingsLoaded === true && codexPlusSettings().reasoningEffortUnlock === true;
  }

  function codexPlusReasoningEffortUnlockTick() {
    if (!codexPlusReasoningEffortUnlockEnabled()) return;
    for (const client of codexPlusOfflineStatsigClients()) {
      try {
        if (codexPlusOfflineStatsigPatch(client, { [codexPlusMultiAgentModeGate]: true })) {
          console.log("[ReasoningEffortUnlock]", "Statsig 没有远程数据，已补开 Ultra 档位（multiAgentMode）");
        }
      } catch (error) {
        console.warn("[ReasoningEffortUnlock]", "补开失败：", error && error.message);
      }
    }
  }

  function codexPlusReasoningEffortUnlockExtension(ctx) {
    codexPlusReasoningEffortUnlockTick();
    // Statsig 可能晚于注入才 Ready，后台刷新失败也可能再次清空数据：低频复查，补过且仍有数据时是空操作。
    ctx.setInterval(codexPlusReasoningEffortUnlockTick, 3000);
  }
