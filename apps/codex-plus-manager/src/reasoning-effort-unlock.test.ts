import assert from "node:assert/strict";
import { it } from "node:test";
import { readFile } from "node:fs/promises";

/**
 * 「显示 Max / Ultra 档位」的页面侧：国内拉不到 OpenAI 远程开关时补开 Ultra（multiAgentMode，闸门 1186680773）。
 * Max 由 Rust 侧启动前写 config.toml（relay_config 的测试守住）。这里钉死：
 *   1. 只在 Statsig 完全没有数据时补，拿到真实数据（开了代理）就不碰；
 *   2. 只补 multiAgentMode 这一个闸门；和「替我审批」先后补时两个闸门合并，谁也不覆盖谁；
 *   3. 开关关着（或后端设置还没读到）时什么都不做；
 *   4. 设置项、内置拓展、分片清单、页面设置映射几处登记一致，默认开。
 */

const read = (path: string) => readFile(new URL(path, import.meta.url), "utf8");

type FakeClient = {
  loadingStatus: string;
  values: unknown;
  setDataCalls: string[];
  getContext(): { values: unknown; user: Record<string, unknown> };
  dataAdapter: { setData(data: string): void };
  updateUserSync(user: unknown): void;
};

function fakeClient(values: unknown): FakeClient {
  const client: FakeClient = {
    loadingStatus: "Ready",
    values,
    setDataCalls: [],
    getContext() {
      return { values: client.values, user: { userID: "user-1" } };
    },
    dataAdapter: {
      setData(data: string) {
        client.setDataCalls.push(data);
      },
    },
    updateUserSync() {
      client.values = JSON.parse(client.setDataCalls.at(-1) ?? "null");
    },
  };
  return client;
}

async function runtime(client: FakeClient, settings: { loaded: boolean; unlock: boolean; autoReview?: boolean }) {
  // 补数据的通用部分在 33-offline-auto-review.js，本分片复用它；两段按清单顺序拼起来执行。
  const shared = await read("../../../assets/inject/renderer-inject/33-offline-auto-review.js");
  const source = await read("../../../assets/inject/renderer-inject/35-reasoning-effort-unlock.js");
  const window = { __STATSIG__: { firstInstance: client, instance: () => client, instances: { a: client } } };
  const intervals: number[] = [];
  const api = new Function(
    "window", "codexPlusBackendSettingsLoaded", "codexPlusSettings",
    `${shared}\n${source}\nreturn { tick: codexPlusReasoningEffortUnlockTick, extension: codexPlusReasoningEffortUnlockExtension, autoReviewTick: codexPlusOfflineAutoReviewTick, gate: codexPlusMultiAgentModeGate };`,
  )(window, settings.loaded, () => ({ reasoningEffortUnlock: settings.unlock, offlineAutoReview: settings.autoReview === true })) as {
    tick(): void;
    autoReviewTick(): void;
    extension(ctx: { setInterval(fn: () => void, ms: number): void }): void;
    gate: string;
  };
  return { ...api, intervals, start: () => api.extension({ setInterval: (_fn, ms) => intervals.push(ms) }) };
}

it("Statsig 没有数据时只补 multiAgentMode（Ultra）闸门，且只补一次", async () => {
  const client = fakeClient(null);
  const app = await runtime(client, { loaded: true, unlock: true });
  app.start();
  app.tick();
  assert.equal(app.gate, "1186680773");
  assert.equal(client.setDataCalls.length, 1);
  const values = JSON.parse(client.setDataCalls[0]);
  assert.deepEqual(Object.keys(values.feature_gates), ["1186680773"]);
  assert.equal(values.feature_gates["1186680773"].value, true);
  assert.deepEqual(values.dynamic_configs, {});
  assert.deepEqual(app.intervals, [3000]);
});

it("和「替我审批」先后补时合并两个闸门", async () => {
  const client = fakeClient(null);
  const app = await runtime(client, { loaded: true, unlock: true, autoReview: true });
  app.autoReviewTick();
  app.tick();
  const gates = (client.values as { feature_gates: Record<string, { value: boolean }> }).feature_gates;
  assert.equal(gates["3902016271"]?.value, true);
  assert.equal(gates["1186680773"]?.value, true);
});

it("拿到真实数据（开了代理）时完全不碰", async () => {
  const client = fakeClient({ feature_gates: { "1186680773": { value: false } } });
  const app = await runtime(client, { loaded: true, unlock: true });
  app.start();
  assert.equal(client.setDataCalls.length, 0);
});

it("开关关着、或后端设置还没读到时什么都不做", async () => {
  for (const settings of [{ loaded: true, unlock: false }, { loaded: false, unlock: true }]) {
    const client = fakeClient(null);
    const app = await runtime(client, settings);
    app.start();
    assert.equal(client.setDataCalls.length, 0, JSON.stringify(settings));
  }
});

it("设置项、内置拓展、分片清单和页面设置映射登记一致，默认开", async () => {
  const registry = JSON.parse(await read("../../../crates/codex-plus-core/assets/enhancement-settings.json")) as {
    items: Array<{ key: string; builtin?: string; restart?: boolean }>;
  };
  const item = registry.items.find((entry) => entry.key === "codexAppReasoningEffortUnlock");
  assert.equal(item?.builtin, "reasoning-effort-unlock");
  assert.equal(item?.restart, true);
  const catalog = await read("../../../assets/inject/renderer-inject/96-builtin-extensions.js");
  assert.match(catalog, /id: "reasoning-effort-unlock", name: "[^"]+", apply: codexPlusReasoningEffortUnlockExtension/);
  const style = await read("../../../assets/inject/renderer-inject/10-style.js");
  assert.match(style, /reasoningEffortUnlock: "codexAppReasoningEffortUnlock"/);
  assert.match(style, /reasoningEffortUnlock: "reasoning-effort-unlock"/);
  assert.match(style, /reasoningEffortUnlock: true, reportedUsageCompaction/, "页面侧默认开");
  const manifest = JSON.parse(await read("../../../assets/inject/renderer-inject/manifest.json")) as { fragments: Array<{ name: string }> };
  const names = manifest.fragments.map((fragment) => fragment.name);
  assert.ok(names.indexOf("33-offline-auto-review.js") < names.indexOf("35-reasoning-effort-unlock.js"), "依赖 33 里的补数据函数");
  assert.ok(names.indexOf("35-reasoning-effort-unlock.js") < names.indexOf("96-builtin-extensions.js"));
  const app = await read("./App.tsx");
  assert.match(app, /codexAppReasoningEffortUnlock: true,/, "管理器默认开");
});
