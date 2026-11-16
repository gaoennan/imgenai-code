import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { MANAGER_NAVIGATE_EVENT } from "./extension-manager-pages.ts";
import { brandIconKey } from "./provider-brands.ts";

const read = (relative: string) => readFileSync(new URL(relative, import.meta.url), "utf8");

test("「添加供应商」目录最前面是 imgenai，点开跳到 imgenai 账户页而不是手填密钥", () => {
  const catalog = read("./GatewayProviders.tsx");
  assert.ok(catalog.indexOf('key: "imgenai"') >= 0);
  assert.ok(catalog.indexOf('key: "imgenai"') < catalog.indexOf('key: "sponsor"'));
  assert.ok(catalog.indexOf('key: "imgenai"') < catalog.indexOf('key: "subscription"'));
  assert.match(catalog, /const IMGENAI_ACCOUNT_EXTENSION = "bundled:imgenai-account";/);
  assert.match(catalog, /new CustomEvent\(MANAGER_NAVIGATE_EVENT, \{ detail: `ext:\$\{IMGENAI_ACCOUNT_EXTENSION\}` \}\)/);
  // 已登录：拓展写进池里的供应商（来源与 Rust 侧 extension_pool::pool_source 一致），目录里打「已添加」的点。
  assert.match(catalog, /const IMGENAI_POOL_SOURCE = "extension-pool:imgenai-account\/imgenai";/);
  assert.match(catalog, /entry\.source === IMGENAI_POOL_SOURCE/);
  const manifest = JSON.parse(read("../../../assets/extensions/imgenai-account/codexpp.json"));
  assert.equal(manifest.id, "imgenai-account");
  const provider = manifest.providers.find((item: { id: string }) => item.id === "imgenai");
  assert.equal(provider.pool, true);
  assert.equal(provider.requiresKey, true);
});

test("目录里的 imgenai 地址与拓展声明的供应商一致", () => {
  const manifest = JSON.parse(read("../../../assets/extensions/imgenai-account/codexpp.json"));
  const provider = manifest.providers.find((item: { id: string }) => item.id === "imgenai");
  assert.ok(provider);
  assert.match(read("./GatewayProviders.tsx"), new RegExp(`const IMGENAI_BASE_URL = "${provider.baseUrl.replace(/[.]/g, "\\.")}";`));
});

test("App 监听管理器内部的换页请求，走拓展页面 navigate 同一套规则", () => {
  assert.equal(MANAGER_NAVIGATE_EVENT, "codexpp:manager-navigate");
  const app = read("./App.tsx");
  assert.match(app, /window\.addEventListener\(MANAGER_NAVIGATE_EVENT, onNavigate\)/);
  assert.match(app, /window\.removeEventListener\(MANAGER_NAVIGATE_EVENT, onNavigate\)/);
  assert.match(app, /navigateFromExtensionRef\.current\(target\)/);
});

test("imgenai 供应商用自己的图标（按名字或地址都认得出）", () => {
  assert.equal(brandIconKey("imgenai"), "imgenai-color");
  assert.equal(brandIconKey("我的中转", "https://api.imgenai.net/v1"), "imgenai-color");
});
