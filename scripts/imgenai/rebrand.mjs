#!/usr/bin/env node
// imgenai Code 品牌化：把一份源码副本里的 Codex++ 身份换成 imgenai Code。
//
// 只在构建用的临时副本上执行（见 build-macos.sh），开发分支保持与上游一致的写法，合并上游更新时不会到处冲突。
// 换的东西分两类：
//   1. 身份：应用 bundle id、状态目录、配置目录、本地端口、链接协议、Windows 卸载项、更新源——
//      改掉才能和原版 Codex++ 同时安装、互不覆盖，也不会被原版的更新源「更新」回去。
//   2. 显示名：界面、提示、安装器里的「Codex++」统一叫「imgenai Code」。
// 文档（README、CHANGELOG、docs/、许可与第三方声明）不动：许可证与版权声明必须原样保留。
//
// 用法：node scripts/imgenai/rebrand.mjs <源码副本目录> [--update-repo owner/name] [--dry-run]
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const args = process.argv.slice(2);
const root = path.resolve(args.find((arg) => !arg.startsWith("--")) || "");
const dryRun = args.includes("--dry-run");
const updateRepoIndex = args.indexOf("--update-repo");
const updateRepo = updateRepoIndex >= 0 ? args[updateRepoIndex + 1] : process.env.IMGENAI_UPDATE_REPO || "gaoennan/imgenai-code";

if (!root || !fs.existsSync(path.join(root, "Cargo.toml")) || !fs.existsSync(path.join(root, "assets/extensions/imgenai-account"))) {
  console.error("用法：node scripts/imgenai/rebrand.mjs <源码副本目录>（目录里要有 Cargo.toml 和 imgenai 账户拓展）");
  process.exit(2);
}
if (!/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(updateRepo)) {
  console.error(`--update-repo 要写成 owner/name：${updateRepo}`);
  process.exit(2);
}
// 防误伤：开发用的工作区（有 .git 目录、分支是 imgenai）不允许直接改。副本由 build-macos.sh 生成，没有 .git。
if (fs.existsSync(path.join(root, ".git")) && !args.includes("--force-in-place")) {
  console.error("拒绝在带 .git 的工作区里直接改名：请对构建副本执行（build-macos.sh 会生成），或明确加 --force-in-place。");
  process.exit(2);
}

const BRAND = "imgenai Code";
const BUNDLE_ID = "net.imgenai.code";
const STATE_DIR = ".imgenai-code";
const URL_SCHEME = "imgenaicode";
const UNINSTALL_KEY = "imgenaiCode";
const PORT_SHIFT = 200;
const SUPPORT_URL = "https://imgenai.net/";
const AD_LIST_URL = process.env.IMGENAI_AD_LIST_URL || "https://imgenai.net/desktop/ads.json";

/** 不处理的路径：文档、许可、上游第三方素材、本脚本自己、锁文件与构建产物。 */
const SKIP = [
  /^\.git\//,
  /(^|\/)node_modules\//,
  /^target\//,
  /^dist\//,
  /^docs\//,
  /^site\//,
  /^scripts\/imgenai\//,
  // 第三方挂件（大肥鱼，MIT）：代码里有原作者适配时加的界面文字要改名，许可与来源说明原样保留。
  /^assets\/inject\/upstream\/.*\/(LICENSE|PROVENANCE\.md|README\.md)$/,
  /^(LICENSE|THIRD_PARTY_NOTICES\.md|CHANGELOG\.md|HANDOVER\.md|design-qa\.md|AGENTS\.md|CONTRIBUTING\.md)$/,
  /^README[^/]*\.md$/,
  /(^|\/)(Cargo\.lock|package-lock\.json|bun\.lock)$/,
];
const TEXT_EXTENSIONS = new Set([
  ".rs", ".ts", ".tsx", ".js", ".mjs", ".cjs", ".json", ".html", ".css", ".md", ".sh", ".ps1", ".nsi", ".nsh", ".py",
  ".plist", ".toml", ".yml", ".yaml", ".txt", ".xml", ".svg", ".applescript", ".bat", ".cmd", ".iss",
]);

/** 按顺序执行；前面的先换掉更长、更具体的写法，后面的通用规则就不会误伤它们。 */
const RULES = [
  // 身份
  { name: "bundle id", from: /com\.bigpizzav3\.codexplusplus/g, to: BUNDLE_ID },
  { name: "状态目录", from: /\.codex-session-delete/g, to: STATE_DIR },
  { name: "链接协议", from: /codexplusplus:\/\//g, to: `${URL_SCHEME}://` },
  { name: "链接协议（plist）", from: /<string>codexplusplus<\/string>/g, to: `<string>${URL_SCHEME}</string>` },
  // 注册表路径有两种写法：原样字符串 / NSIS 里是单反斜杠，Rust 普通字符串里是转义后的双反斜杠。
  { name: "链接协议（注册表，转义）", from: /Classes\\\\codexplusplus/g, to: `Classes\\\\${URL_SCHEME}` },
  { name: "链接协议（注册表）", from: /Classes\\codexplusplus/g, to: `Classes\\${URL_SCHEME}` },
  { name: "Windows 卸载项（转义）", from: /Uninstall\\\\CodexPlusPlus/g, to: `Uninstall\\\\${UNINSTALL_KEY}` },
  { name: "Windows 卸载项", from: /Uninstall\\CodexPlusPlus/g, to: `Uninstall\\${UNINSTALL_KEY}` },
  { name: "安装包文件名", from: /CodexPlusPlus-(\$\{VERSION\}|\*|\$VERSION)/g, to: `imgenaiCode-$1` },
  { name: "安装器发布者", from: /"Publisher" "BigPizzaV3"/g, to: '"Publisher" "imgenai"' },
  // 安装包文件属性里的公司名跟发布者一致；版权一栏是原作者署名，保留不改。
  { name: "安装器公司名", from: /"CompanyName" "BigPizzaV3"/g, to: '"CompanyName" "imgenai"' },
  // 广告源也出现在注入 Codex 页面的脚本里（页面自己直接拉），不只 ads.rs，所以全局换。
  ...adListRules("广告源（全局）"),
  {
    name: "本地端口",
    from: /(?<![0-9])573(19|2[0-3])(?![0-9])/g,
    to: (match) => String(Number(match) + PORT_SHIFT),
  },
  // 显示名（也覆盖配置目录、快捷方式、应用包名这些用名字拼出来的路径）
  // 测试与源码里有正则写法：Rust 普通字符串里是 Codex\\+\\+，JS 正则字面量里是 Codex\+\+，要和字面量一起换掉。
  { name: "显示名（Rust 正则转义）", from: /Codex\\\\\+\\\\\+/g, to: BRAND },
  { name: "显示名（正则转义）", from: /Codex\\\+\\\+/g, to: BRAND },
  { name: "显示名", from: /Codex\+\+/g, to: BRAND },
  // 小写写法用来认出「自己」的安装目录（app_paths、outbound、agent_cache 等都按小写比对）。
  // 不换的话改名后的程序认不出自己，会把自己当成官方 Codex App。
  { name: "显示名（小写比对）", from: /codex\+\+/g, to: BRAND.toLowerCase() },
];

/** 只对个别文件做的替换（写法太通用，全局换会误伤）。 */
const FILE_RULES = {
  "crates/codex-plus-core/src/update.rs": [
    { name: "更新源仓库", from: /"BigPizzaV3\/CodexPlusPlus"/g, to: JSON.stringify(updateRepo) },
    { name: "更新源 latest.json", from: /https:\/\/github\.com\/BigPizzaV3\/CodexPlusPlus\/releases/g, to: `https://github.com/${updateRepo}/releases` },
    // 自动更新从发布附件里挑安装包时要求文件名同时含 codex 和 plus；我们的安装包叫 imgenaiCode-…，换成认 imgenai。
    {
      name: "更新包文件名识别",
      from: /(\w+)\.contains\("codex"\)\s*&&\s*\1\.contains\("plus"\)/g,
      to: (_match, variable) => `${variable}.contains("imgenai")`,
    },
    { name: "更新包测试文件名", from: /CodexPlusPlus([-_])(\d)/g, to: (_match, separator, digit) => `imgenaiCode${separator}${digit}` },
  ],
  "crates/codex-plus-core/src/install/windows.rs": [
    { name: "Windows 卸载项名", from: /uninstall_key: "CodexPlusPlus"/g, to: `uninstall_key: "${UNINSTALL_KEY}"` },
  ],
  // 测试里写死的旧值，跟着身份一起换，测试才能守住改名后的行为。
  "crates/codex-plus-core/tests/all/installers.rs": [
    { name: "测试：卸载项名", from: /uninstall_key, "CodexPlusPlus"/g, to: `uninstall_key, "${UNINSTALL_KEY}"` },
    { name: "测试：读取的卸载项", from: /for key in \["CodexPlusPlus", /g, to: `for key in ["${UNINSTALL_KEY}", ` },
  ],
  "crates/codex-plus-core/src/gateway/accounts.rs": [
    // 「用 ChatGPT 登录」授权页显示的应用名（URL 编码，空格是 %20）。
    { name: "测试：授权页应用名", from: /agent_name_hint=Codex%2B%2B/g, to: `agent_name_hint=${encodeURIComponent(BRAND)}` },
  ],
  // 首页「推荐 / 赞助商」广告：原版从上游作者的广告服务器拉其他中转站的广告，换成 imgenai 自己的地址
  // （文件还没放上去时拉取失败，首页广告位为空）。
  "crates/codex-plus-core/src/ads.rs": adListRules("广告源"),
  "crates/codex-plus-core/tests/all/ads.rs": adListRules("测试：广告源"),
};

function adListRules(label) {
  return [
    { name: `${label} 1`, from: /https:\/\/ads\.codexpp\.cc\/ads\.json/g, to: AD_LIST_URL },
    { name: `${label} 2`, from: /https:\/\/raw\.githubusercontent\.com\/BigPizzaV3\/Ad-List\/main\/ads\.json/g, to: AD_LIST_URL },
    { name: `${label} 3`, from: /https:\/\/cdn\.jsdelivr\.net\/gh\/BigPizzaV3\/Ad-List@main\/ads\.json/g, to: AD_LIST_URL },
  ];
}

/** 显示名换完之后再做的替换：这里写进去的「Codex++」是对原项目的署名，不能再被改名。 */
const UPSTREAM_ATTRIBUTION = "基于开源项目 Codex++（AGPL-3.0）修改";
const ABOUT_INJECT_RULES = [
  {
    name: "Codex 设置页：关于里的署名",
    from: /GitHub: (<a href="https:\/\/github\.com\/BigPizzaV3\/CodexPlusPlus"[^>]*>[^<]*<\/a>)<br>Discord: <a [^>]*>[^<]*<\/a><br>Telegram: <a [^>]*>[^<]*<\/a>/g,
    // 署名原项目，源码链接给本仓库（AGPL：向用户提供修改后版本的源码）。
    to: `${UPSTREAM_ATTRIBUTION}，源码：<a href="https://github.com/${updateRepo}" target="_blank" rel="noreferrer">https://github.com/${updateRepo}</a>`,
  },
  { name: "Codex 设置页：反馈地址", from: /"https:\/\/github\.com\/BigPizzaV3\/CodexPlusPlus\/issues"/g, to: JSON.stringify(SUPPORT_URL) },
];
const POST_FILE_RULES = {
  "apps/codex-plus-manager/src/App.tsx": [
    {
      name: "管理器：关于里的署名",
      from: /detail="github\.com\/BigPizzaV3\/CodexPlusPlus" onClick=\{\(\) => openUrl\("https:\/\/github\.com\/BigPizzaV3\/CodexPlusPlus"\)\}/g,
      to: `detail="${UPSTREAM_ATTRIBUTION} · 源码 github.com/${updateRepo}" onClick={() => openUrl("https://github.com/${updateRepo}")}`,
    },
    {
      name: "管理器：反馈问题",
      from: /detail=\{t\("在 GitHub Issues 报告问题或提建议"\)\} icon=\{<Github \/>\} onClick=\{\(\) => openUrl\("https:\/\/github\.com\/BigPizzaV3\/CodexPlusPlus\/issues"\)\}/g,
      to: `detail="到 imgenai 官网联系客服" icon={<Github />} onClick={() => openUrl(${JSON.stringify(SUPPORT_URL)})}`,
    },
    { name: "管理器：去掉上游 Discord", from: /\s*<CxRow detail="discord\.gg\/y96kX7A76v"[\s\S]*?<\/CxRow>/g, to: "" },
    { name: "管理器：去掉上游 Telegram", from: /\s*<CxRow detail="t\.me\/CodexPlusPlus"[\s\S]*?<\/CxRow>/g, to: "" },
    // 检查更新先走国内下载站（imgenai_update_mirror.rs），设置页的说明跟着改。
    { name: "管理器：更新来源说明", from: /t\("从 GitHub Release 下载安装包更新"\)/g, to: 't("从 imgenai 下载站下载安装包更新")' },
  ],
  "apps/codex-plus-manager/src/i18n-en.ts": [
    {
      name: "英文：更新来源说明",
      from: /"从 GitHub Release 下载安装包更新": "Updates are downloaded from GitHub Releases",/g,
      to: '"从 imgenai 下载站下载安装包更新": "Updates are downloaded from the imgenai mirror, with GitHub Releases as a fallback",',
    },
  ],
  // macOS 菜单栏托盘图标：原版按「偏橙程度」从橙色 logo 抠出模板图；imgenai 图标是紫色，照原算法会整张透明、托盘里看不见。
  // 改按「偏紫程度」（B - G）：紫色六边形保留，白色圆点与浅色底板抠空（取值见 brand/make-icons.py 生成的图标）。
  "apps/codex-plus-manager/src-tauri/src/lib.rs": [
    {
      name: "托盘模板：按偏紫程度取 alpha",
      from: /let warmth = pixel\[0\]\.saturating_sub\(pixel\[2\]\) as u32;(\s*)let coverage = warmth\.saturating_sub\(40\)\.min\(60\) \* 255 \/ 60;/g,
      to: "let purple = pixel[2].saturating_sub(pixel[1]) as u32;$1let coverage = purple.saturating_sub(60).min(60) * 255 / 60;",
    },
    { name: "托盘模板：注释", from: /按「偏橙程度」（R - B）重算 alpha：橙色块保留，/g, to: "按「偏紫程度」（B - G）重算 alpha：紫色六边形保留，" },
    { name: "托盘模板测试：坐标说明", from: /前方块正面左侧为橙色，前方十字中心为白色。/g, to: "六边形左侧为紫色，中心圆点为白色。" },
    {
      name: "托盘模板测试：不透明处",
      from: /assert_eq!\(alpha_at\(at\(0\.15\), at\(0\.5\)\), 255, "橙色方块应不透明"\);/g,
      to: 'assert_eq!(alpha_at(at(0.30), at(0.5)), 255, "紫色六边形应不透明");',
    },
    {
      name: "托盘模板测试：抠空处",
      from: /assert_eq!\(alpha_at\(at\(0\.37\), at\(0\.64\)\), 0, "白十字应抠空"\);/g,
      to: 'assert_eq!(alpha_at(at(0.5), at(0.5)), 0, "中心圆点应抠空");',
    },
  ],
  "assets/inject/renderer-inject/40-backend-settings.js": ABOUT_INJECT_RULES,
  "assets/inject/renderer-inject.js": ABOUT_INJECT_RULES.map((rule) => ({ ...rule, name: `${rule.name}（组装产物）` })),
};

// 上游的赞助商（给上游作者付费推广的同行中转站）：「添加供应商」目录不再单列，预设与只测它们的测试一并删掉。
// imgenai 自己的入口是 GatewayProviders.tsx 里的 imgenai 分组（点开账户页登录，密钥由拓展自动同步）。
// 赞助商名单上游经常换，这里从副本里现读，不写死。
const SPONSORS_FILE = "apps/codex-plus-manager/src/gateway-model.ts";
const SPONSORS_DECL = /(export const SPONSORS: Array<\{ id: string; expiresAt: string \}> = )\[[\s\S]*?\n\];/;
const sponsorIds = (() => {
  const file = path.join(root, SPONSORS_FILE);
  const block = fs.existsSync(file) ? fs.readFileSync(file, "utf8").match(SPONSORS_DECL)?.[0] ?? "" : "";
  return [...block.matchAll(/id: "([^"]+)"/g)].map((match) => match[1]);
})();
const sponsorAlternation = sponsorIds.map((id) => id.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")).join("|");
POST_FILE_RULES[SPONSORS_FILE] = [
  { name: "去掉上游赞助商名单", from: new RegExp(SPONSORS_DECL.source, "g"), to: "$1[];" },
];
if (sponsorIds.length) {
  POST_FILE_RULES["apps/codex-plus-manager/src/presets.ts"] = [
    { name: "删掉上游赞助商的预设", from: new RegExp(`\\n  \\{\\n    id: "(?:${sponsorAlternation})",[\\s\\S]*?\\n  \\},`, "g"), to: "" },
  ];
  // 只删「提到赞助商 id 的那几个测试」：每个测试从 test(" 开始、到行首的 }); 结束，不跨测试匹配。
  const sponsorTests = { name: "删掉只测上游赞助商的测试", from: new RegExp(`\\ntest\\("[^\\n]*\\{(?:(?!\\n\\}\\);)[\\s\\S])*?"(?:${sponsorAlternation})"[\\s\\S]*?\\n\\}\\);`, "g"), to: "" };
  POST_FILE_RULES["apps/codex-plus-manager/src/gateway-model.test.ts"] = [sponsorTests];
  POST_FILE_RULES["apps/codex-plus-manager/src/presets.test.ts"] = [{ ...sponsorTests, name: `${sponsorTests.name}（预设）` }];
}

function walk(dir, out = []) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    const relative = path.relative(root, full).split(path.sep).join("/");
    if (SKIP.some((pattern) => pattern.test(entry.isDirectory() ? `${relative}/` : relative))) continue;
    if (entry.isDirectory()) walk(full, out);
    else if (entry.isFile() && TEXT_EXTENSIONS.has(path.extname(entry.name).toLowerCase())) out.push(relative);
  }
  return out;
}

const counts = new Map();
let changedFiles = 0;
for (const relative of walk(root)) {
  const full = path.join(root, relative);
  const original = fs.readFileSync(full, "utf8");
  let text = original;
  const rules = [...(FILE_RULES[relative] || []), ...RULES, ...(POST_FILE_RULES[relative] || [])];
  for (const rule of rules) {
    text = text.replace(rule.from, (...match) => {
      counts.set(rule.name, (counts.get(rule.name) || 0) + 1);
      return typeof rule.to === "function" ? rule.to(...match) : match[0].replace(rule.from, rule.to);
    });
    rule.from.lastIndex = 0;
  }
  if (text !== original) {
    changedFiles += 1;
    if (!dryRun) fs.writeFileSync(full, text);
  }
}

// 每条文件专属规则都必须命中：上游改了写法就让构建失败，而不是悄悄漏改（比如更新源没换掉就会被「更新」回原版）。
const missing = [...Object.values(FILE_RULES), ...Object.values(POST_FILE_RULES)].flat().filter((rule) => !counts.get(rule.name));
// 品牌图片：scripts/imgenai/brand/ 里的文件按下表覆盖进副本（应用图标、托盘模板的来源图、界面 logo）。
// 路径里可能有中文：用 fileURLToPath 解码，别直接取 URL 的 pathname（那是百分号编码的）。
const brandDir = path.join(path.dirname(fileURLToPath(import.meta.url)), "brand");
const BRAND_FILES = {
  "icon-macos.png": "apps/codex-plus-manager/src-tauri/icons/icon-macos.png",
  "icon.png": "apps/codex-plus-manager/src-tauri/icons/icon.png",
  "icon.ico": "apps/codex-plus-manager/src-tauri/icons/icon.ico",
  "app-logo.png": "apps/codex-plus-manager/src/assets/codex-plus-logo.png",
};
const icons = Object.keys(BRAND_FILES).filter((name) => fs.existsSync(path.join(brandDir, name)));
if (!dryRun) for (const name of icons) fs.copyFileSync(path.join(brandDir, name), path.join(root, BRAND_FILES[name]));

console.log(`${dryRun ? "[预演] " : ""}改名完成：${changedFiles} 个文件，更新源 ${updateRepo}`);
for (const [name, count] of counts) console.log(`  ${name}: ${count}`);
console.log(`  品牌图标: ${icons.length ? icons.join("、") : "无（沿用原图标）"}`);
if (missing.length) {
  console.error(`以下必改项没有命中，上游可能改了写法，请更新 rebrand.mjs：${missing.map((rule) => rule.name).join("、")}`);
  process.exit(1);
}
