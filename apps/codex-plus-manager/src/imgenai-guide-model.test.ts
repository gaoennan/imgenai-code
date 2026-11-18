import assert from "node:assert/strict";
import test from "node:test";
import { blankBinding, type GatewayPayload } from "./gateway-model.ts";
import {
  codexUsesImgenai,
  guideCardVisible,
  guideProgress,
  imgenaiCodexBinding,
  IMGENAI_POOL_SOURCE,
  parseGuideRecord,
  recommendedModel,
  serializeGuideRecord,
  type GuideFacts,
} from "./imgenai-guide-model.ts";

/** 已登录、有订阅、Codex 已接入且默认模型发到 imgenai。 */
function payload(): GatewayPayload {
  return {
    gateway: {
      version: 1,
      bindings: { codex: { ...blankBinding(), enabled: true, model: "imgenai/gpt-6.2-sol" } },
      keyPolicy: { mode: "sticky" },
      restoreOnExit: false,
    },
    port: 57321,
    helperRunning: true,
    pool: [
      { id: "mykey", name: "My OpenAI", baseUrl: "https://api.openai.com/v1", protocol: "responses", official: false, models: ["gpt-5"], keyCount: 1 },
      { id: "imgenai", name: "imgenai", baseUrl: "https://api.imgenai.net/v1", protocol: "responses", official: false, models: ["gpt-6.2-sol", "gpt-6.2-astra", "codex-auto-review"], keyCount: 1, source: IMGENAI_POOL_SOURCE },
    ],
    groups: [],
    agents: [{ agent: "codex", name: "Codex", enabled: true, baseUrl: "http://127.0.0.1:57321", defaultModel: "imgenai/gpt-6.2-sol", models: ["imgenai/gpt-6.2-sol"] }],
    health: { keys: [], providers: [] },
    codexProviders: [],
    claudeProviders: [],
    grokProfiles: [],
  };
}

const facts = (overrides: Partial<GuideFacts> = {}): GuideFacts => ({
  focus: "codex", codexInstalled: true, loggedIn: true, payload: payload(), codexRunning: false, ...overrides,
});

test("按真实状态一步步往下走：登录 → 开通 → 接入 → 启动 → 完成", () => {
  const noKey = payload();
  noKey.pool = noKey.pool.filter((entry) => entry.source !== IMGENAI_POOL_SOURCE);
  noKey.agents[0].enabled = false;
  noKey.gateway.bindings.codex = { ...blankBinding() };
  assert.equal(guideProgress(facts({ loggedIn: false, payload: noKey })).current, "login");
  assert.equal(guideProgress(facts({ payload: noKey })).current, "subscribe");

  const unbound = payload();
  unbound.agents[0].enabled = false;
  unbound.gateway.bindings.codex = { ...blankBinding(), enabled: false, model: "imgenai/gpt-6.2-sol" };
  assert.equal(guideProgress(facts({ payload: unbound })).current, "connect");

  const ready = guideProgress(facts());
  assert.equal(ready.current, "launch");
  assert.equal(ready.setupDone, true);
  assert.equal(ready.count, 3);

  const running = guideProgress(facts({ codexRunning: true }));
  assert.equal(running.current, "done");
  assert.equal(running.count, 4);
});

test("没登录时不认池里残留的 imgenai，后面的步骤也不打勾", () => {
  const progress = guideProgress(facts({ loggedIn: false, codexRunning: true }));
  assert.equal(progress.current, "login");
  assert.deepEqual(progress.done, { login: false, subscribe: false, connect: false, launch: false });
});

test("默认模型发到别家不算接入 imgenai", () => {
  const other = payload();
  other.gateway.bindings.codex = { ...blankBinding(), enabled: true, model: "gpt-5" };
  other.agents[0].defaultModel = "gpt-5";
  assert.equal(codexUsesImgenai(other), false);
  assert.equal(guideProgress(facts({ payload: other })).current, "connect");
  assert.equal(codexUsesImgenai(payload()), true);
  assert.equal(codexUsesImgenai(null), false);
});

test("只用增强功能和没装 Codex 会拦在最前面", () => {
  assert.equal(guideProgress(facts({ focus: "enhance" })).current, "mode");
  assert.equal(guideProgress(facts({ codexInstalled: false })).current, "install");
  // 还没读到是否安装时不拦。
  assert.equal(guideProgress(facts({ codexInstalled: null })).current, "launch");
});

test("状态没读全时不下结论，进度卡先不出", () => {
  for (const missing of [{ loggedIn: null }, { payload: null }, { codexInstalled: null }] as Partial<GuideFacts>[]) {
    const progress = guideProgress(facts({ ...missing, loggedIn: missing.loggedIn === null ? null : false }));
    assert.equal(progress.loading, true);
    assert.equal(guideCardVisible(null, progress, true), false);
  }
});

test("进度卡：配好前一直在；这次见过没配好的，要等启动过 Codex 才收起", () => {
  const fresh = guideProgress(facts({ loggedIn: false }));
  assert.equal(guideCardVisible(null, fresh, true), true);
  assert.equal(guideCardVisible("dismissed", fresh, true), false);
  const ready = guideProgress(facts());
  assert.equal(guideCardVisible(null, ready, true), true);
  assert.equal(guideCardVisible(null, ready, false), false);
  assert.equal(guideCardVisible(null, guideProgress(facts({ codexRunning: true })), true), false);
});

test("一键接入挑 imgenai 列表里第一个聊天模型", () => {
  const entry = payload().pool[1];
  assert.equal(recommendedModel(entry), "gpt-6.2-sol");
  assert.equal(recommendedModel({ ...entry, models: ["codex-auto-review", "gpt-image-2", "gpt-6-luna"] }), "gpt-6-luna");
  assert.equal(recommendedModel({ ...entry, models: ["x-pic", "gpt-6-sol"], imageModels: ["x-pic"] }), "gpt-6-sol");
  assert.equal(recommendedModel({ ...entry, models: ["codex-auto-review"] }), null);
  assert.equal(recommendedModel(undefined), null);
});

test("完成 / 关掉的记录可读回，坏记录按没有处理", () => {
  for (const state of ["completed", "dismissed"] as const) assert.equal(parseGuideRecord(serializeGuideRecord(state)), state);
  assert.equal(parseGuideRecord(null), null);
  assert.equal(parseGuideRecord("broken"), null);
  assert.equal(parseGuideRecord('{"version":1,"state":"open"}'), null);
  assert.equal(parseGuideRecord('{"version":0,"state":"completed"}'), null);
});

test("指引要圈的元素在界面源码里都有锚点", async () => {
  const { readFileSync } = await import("node:fs");
  const read = (name: string) => readFileSync(new URL(name, import.meta.url), "utf8");
  const app = read("./App.tsx");
  const agents = read("./GatewayAgents.tsx");
  const guide = read("./ImgenaiGuide.tsx");
  assert.match(app, /data-guide="codex-run"/);
  assert.match(app, /data-extension-page=\{page\.key\}/);
  assert.match(app, /data-nav=\{item\.key\}/);
  assert.match(app, /key: "gateway", route: "gateway"/);
  assert.match(agents, /data-agent-row=\{agent\.agent\}/);
  assert.match(agents, /className="gw-picker-button"/);
  for (const selector of ['[data-guide="codex-run"]', '[data-nav="gateway"]', '[data-agent-row="codex"] .gw-picker-button', "[data-extension-page="]) {
    assert.ok(guide.includes(selector), `指引里没用到 ${selector}`);
  }
});

test("一键接入写的绑定和在选择器里点 imgenai 下的模型一样，并把 imgenai 放回 Codex 的供应商范围", () => {
  const data = payload();
  data.gateway.bindings.codex = { ...blankBinding(), enabled: false, model: "", poolExclude: ["imgenai", "mykey"], token: "keep-me" };
  data.agents[0].enabled = false;
  const plan = imgenaiCodexBinding(data);
  assert.ok("binding" in plan);
  assert.equal(plan.model, "gpt-6.2-sol");
  assert.equal(plan.binding.model, "imgenai/gpt-6.2-sol");
  assert.deepEqual(plan.binding.defaultModelSelection, { providerId: "imgenai", upstreamModel: "gpt-6.2-sol", providerSource: IMGENAI_POOL_SOURCE });
  assert.deepEqual(plan.binding.poolExclude, ["mykey"]);
  // 其余字段沿用原绑定。
  assert.equal(plan.binding.token, "keep-me");

  // 没有绑定（从没接过）也能建。
  const fresh = payload();
  delete fresh.gateway.bindings.codex;
  const created = imgenaiCodexBinding(fresh);
  assert.ok("binding" in created);
  assert.equal(created.binding.model, "imgenai/gpt-6.2-sol");

  const noKey = payload();
  noKey.pool = noKey.pool.filter((entry) => entry.source !== IMGENAI_POOL_SOURCE);
  assert.ok("error" in imgenaiCodexBinding(noKey));
  assert.ok("error" in imgenaiCodexBinding(null));
});
