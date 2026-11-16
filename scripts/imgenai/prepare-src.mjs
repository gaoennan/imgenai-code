#!/usr/bin/env node
// 准备 imgenai Code 的构建副本（macOS / Windows / CI 通用）：
//   1. 把当前工作区（git 跟踪的文件 + 未提交的新文件，不含被忽略的）复制到 <目标目录>；
//   2. 把 ClaudeMom 依赖（claude-local，上游作者的另一个仓库）复制进副本，作为工作区成员并用 [patch] 指向它；
//   3. 执行 rebrand.mjs 改名；给了 --version 时把所有带版本号的文件改成这个版本。
//
// 用法：node scripts/imgenai/prepare-src.mjs <目标目录> [--version 1.0.0] [--update-repo owner/name]
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, "../..");
const args = process.argv.slice(2);
const dest = path.resolve(args.find((arg) => !arg.startsWith("--")) || "");
const option = (name) => {
  const index = args.indexOf(name);
  return index >= 0 ? args[index + 1] : undefined;
};
const version = option("--version");
const updateRepo = option("--update-repo") || process.env.IMGENAI_UPDATE_REPO;

// 构建副本放在仓库外：仓库在 iCloud 同步的桌面上，副本加编译产物有好几个 GB。
if (!dest || dest === root || dest.startsWith(`${root}${path.sep}`) || root.startsWith(`${dest}${path.sep}`)) {
  console.error("用法：node scripts/imgenai/prepare-src.mjs <仓库外的目标目录> [--version x.y.z] [--update-repo owner/name]");
  process.exit(2);
}
if (version && !/^\d+\.\d+\.\d+$/.test(version)) {
  console.error(`--version 要写成 x.y.z：${version}`);
  process.exit(2);
}

const log = (message) => console.error(`==> ${message}`);
const run = (command, commandArgs, options = {}) => execFileSync(command, commandArgs, { encoding: "utf8", maxBuffer: 256 * 1024 * 1024, ...options });

// 1. 复制源码
log(`复制源码到 ${dest}`);
fs.rmSync(dest, { recursive: true, force: true });
const files = run("git", ["ls-files", "-co", "--exclude-standard", "-z"], { cwd: root }).split("\0").filter(Boolean);
for (const relative of files) {
  const from = path.join(root, relative);
  const stat = fs.lstatSync(from, { throwIfNoEntry: false });
  if (!stat || stat.isDirectory()) continue; // 已删除未提交的文件、子模块目录
  const to = path.join(dest, relative);
  fs.mkdirSync(path.dirname(to), { recursive: true });
  if (stat.isSymbolicLink()) fs.symlinkSync(fs.readlinkSync(from), to);
  else fs.copyFileSync(from, to);
}

// 2. ClaudeMom 依赖：版本号钉成原依赖的版本，否则 Cargo.lock 锁着的旧版本不匹配，[patch] 不生效。
log("复制 ClaudeMom 依赖（claude-local）");
const metadata = JSON.parse(run("cargo", ["metadata", "--format-version", "1", "--locked"], { cwd: dest }));
const claudeLocal = metadata.packages.find((pkg) => pkg.name === "claude-local");
if (!claudeLocal) throw new Error("cargo metadata 里没有 claude-local：上游可能改了依赖，请更新 prepare-src.mjs");
const vendorDir = path.join(dest, "vendor", "claude-local");
fs.cpSync(path.dirname(claudeLocal.manifest_path), vendorDir, { recursive: true });
const workspaceToml = path.join(dest, "Cargo.toml");
let workspaceText = fs.readFileSync(workspaceToml, "utf8");
if (!/members = \[\r?\n/.test(workspaceText)) throw new Error("Cargo.toml 的 members 写法变了，请更新 prepare-src.mjs");
workspaceText = workspaceText.replace(/members = \[(\r?\n)/, `members = [$1  "vendor/claude-local",$1`);
workspaceText += `\n[patch."https://github.com/BigPizzaV3/ClaudeMom"]\nclaude-local = { path = "vendor/claude-local" }\n`;
fs.writeFileSync(workspaceToml, workspaceText);
const memberToml = path.join(vendorDir, "Cargo.toml");
fs.writeFileSync(memberToml, fs.readFileSync(memberToml, "utf8").replace("version.workspace = true", `version = "${claudeLocal.version}"`));

// 3. 改名
log("改名为 imgenai Code");
run(process.execPath, [path.join(here, "rebrand.mjs"), dest, ...(updateRepo ? ["--update-repo", updateRepo] : [])], { stdio: "inherit" });

// 4. 版本号：用上游发版脚本登记的文件清单，一处都不漏（Cargo.toml、Cargo.lock、tauri.conf.json、package.json…）。
if (version) {
  const { VERSION_FILES } = await import(pathToFileURL(path.join(dest, "scripts", "release-meta.mjs")).href);
  for (const { file, read, bump } of VERSION_FILES) {
    const target = path.join(dest, file);
    // 上游的读写规则按 LF 写；检出成 CRLF 时先换成 LF 处理，写回时再还原。
    const raw = fs.readFileSync(target, "utf8");
    const crlf = raw.includes("\r\n");
    const text = crlf ? raw.replace(/\r\n/g, "\n") : raw;
    const from = read(text);
    if (!from) throw new Error(`读不到 ${file} 里的版本号`);
    const bumped = bump(text, from, version);
    if (read(bumped) !== version) throw new Error(`${file} 的版本号没改成 ${version}`);
    fs.writeFileSync(target, crlf ? bumped.replace(/\n/g, "\r\n") : bumped);
  }
  log(`版本号改为 ${version}（${VERSION_FILES.map((entry) => entry.file).join("、")}）`);
}
