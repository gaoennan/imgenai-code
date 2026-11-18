/**
 * imgenai 上手指引：按客户端的真实状态算出「现在该做哪一步」，界面只负责高亮和摆放说明卡。
 *
 * 四步：登录 imgenai 账户 → 开通订阅或免费试用 → 让 Codex 用 imgenai → 启动 Codex。
 * 前面可能多出两步拦路的：使用模式是「只用增强功能」（看不到网关和账户页）、本机没装 Codex。
 * 每一步都看真实状态打勾，用户在别处做完了也算，不靠点过哪个按钮。
 */
import type { AgentFocus } from "./agent-focus-model.ts";
import { bindingFromDraft, draftFromBinding, pickerSections, validateDraft, type AgentBinding, type GatewayPayload, type GatewayPoolEntry } from "./gateway-model.ts";
import { setupGatewayState } from "./onboarding-setup-model.ts";

/** 内置拓展「imgenai 账户」的 key 与它写进供应商池的来源（见 assets/extensions/imgenai-account/codexpp.json）。 */
export const IMGENAI_ACCOUNT_KEY = "bundled:imgenai-account";
export const IMGENAI_POOL_SOURCE = "extension-pool:imgenai-account/imgenai";

export const GUIDE_STORAGE_KEY = "imgenai.guide";
const GUIDE_VERSION = 1;

export type GuideStepId = "mode" | "install" | "login" | "subscribe" | "connect" | "launch" | "done";
/** 清单上固定显示的四步；mode / install 只在拦路时插到最前面。 */
export const MAIN_STEPS = ["login", "subscribe", "connect", "launch"] as const;
export type MainStep = (typeof MAIN_STEPS)[number];

export type GuideFacts = {
  focus: AgentFocus;
  /** null：还没读到。 */
  codexInstalled: boolean | null;
  /** null：还没读到。 */
  loggedIn: boolean | null;
  /** null：还没读到。 */
  payload: GatewayPayload | null;
  codexRunning: boolean;
};

/** 指引面板里可以前后翻看的页：四步，配好了再加最后一页「怎么用」。 */
export function guideSequence(setupDone: boolean): GuideStepId[] {
  return setupDone ? [...MAIN_STEPS, "done"] : [...MAIN_STEPS];
}

/** 前后翻页：`delta` 为 -1 / 1；不在序列里（如拦路的 mode / install）时，往后翻到第一页。到头返回 null。 */
export function neighborStep(sequence: GuideStepId[], from: GuideStepId, delta: -1 | 1): GuideStepId | null {
  const index = sequence.indexOf(from);
  if (index < 0) return delta > 0 ? sequence[0] ?? null : null;
  return sequence[index + delta] ?? null;
}

export type GuideProgress = {
  current: GuideStepId;
  /** 还有状态没读到：先不下结论，不自动弹出。 */
  loading: boolean;
  done: Record<MainStep, boolean>;
  /** 前三步（登录、开通、接入）都好了：老用户就是这样，不再打扰。 */
  setupDone: boolean;
  /** 四步里完成了几步。 */
  count: number;
};

/** 供应商池里由 imgenai 账户写进来的那一家：登录且拿到密钥（有订阅）后才有。 */
export function imgenaiEntry(payload: GatewayPayload | null): GatewayPoolEntry | undefined {
  return payload?.pool.find((entry) => entry.source === IMGENAI_POOL_SOURCE);
}

/** Codex 已接入网关，且默认模型实际发到 imgenai。默认模型发到别家（如用户自己的 Key）不算。 */
export function codexUsesImgenai(payload: GatewayPayload | null): boolean {
  const entry = imgenaiEntry(payload);
  if (!payload || !entry) return false;
  const state = setupGatewayState("codex", payload, "codex");
  return state.bindingReady && state.candidates.some((candidate) => candidate.providerId === entry.id);
}

export function guideProgress(facts: GuideFacts): GuideProgress {
  const login = facts.loggedIn === true;
  const subscribe = login && Boolean(imgenaiEntry(facts.payload));
  const connect = subscribe && codexUsesImgenai(facts.payload);
  const launch = connect && facts.codexRunning;
  const done = { login, subscribe, connect, launch };
  const loading = facts.loggedIn === null || facts.payload === null || facts.codexInstalled === null;
  const current: GuideStepId = facts.focus === "enhance" ? "mode"
    : facts.codexInstalled === false ? "install"
    : !login ? "login"
    : !subscribe ? "subscribe"
    : !connect ? "connect"
    : !launch ? "launch"
    : "done";
  return { current, loading, done, setupDone: connect, count: [login, subscribe, connect, launch].filter(Boolean).length };
}

/** 一键接入时给 Codex 选的默认模型：imgenai 列表里第一个聊天模型（列表顺序由拓展声明，最新的主力模型在前）。 */
export function recommendedModel(entry: GatewayPoolEntry | undefined): string | null {
  if (!entry) return null;
  const images = new Set((entry.imageModels ?? []).map((model) => model.toLowerCase()));
  return entry.models.find((model) => {
    const lower = model.toLowerCase();
    return !images.has(lower) && !lower.includes("auto-review") && !lower.includes("image");
  }) ?? null;
}

/**
 * 一键接入要写的 Codex 绑定：和在「Codex 接入」的模型选择器里点 imgenai 下的这个模型完全一样
 * （默认模型写成 `<供应商 id>/<模型>` 并记下来源），并确保 imgenai 没被排除在 Codex 的供应商范围外。
 * 返回 `{ error }` 时是界面上要提示的原因；`manual` 表示该带用户去选择器里手动选。
 */
export function imgenaiCodexBinding(payload: GatewayPayload | null):
  | { binding: AgentBinding; model: string }
  | { error: string; manual?: boolean } {
  const entry = imgenaiEntry(payload);
  const model = recommendedModel(entry);
  if (!payload || !entry || !model) return { error: "没找到 imgenai 的模型，确认账户页里订阅已开通后再试。" };
  const previous = payload.gateway.bindings.codex;
  const draft = draftFromBinding(previous);
  draft.poolExclude = draft.poolExclude.filter((id) => id !== entry.id);
  const item = pickerSections(payload, "", () => "", draft.poolExclude)
    .flatMap((section) => section.items)
    .find((candidate) => candidate.providerModel === `${entry.id}/${model}`);
  if (!item) return { error: "模型列表里没找到 imgenai，请在 Codex 这一行手动选模型。", manual: true };
  const next = { ...draft, model: item.providerModel ?? item.model };
  const error = validateDraft(next, payload);
  if (error) return { error };
  return { binding: bindingFromDraft(next, previous, item), model };
}

export type GuideRecord = "completed" | "dismissed";

export function parseGuideRecord(stored: string | null): GuideRecord | null {
  try {
    const saved: unknown = JSON.parse(stored ?? "null");
    if (saved && typeof saved === "object" && "version" in saved && "state" in saved
      && typeof saved.version === "number" && saved.version >= GUIDE_VERSION
      && (saved.state === "completed" || saved.state === "dismissed")) return saved.state;
  } catch { /* 记录坏了按没有处理。 */ }
  return null;
}

export function serializeGuideRecord(state: GuideRecord): string {
  return JSON.stringify({ version: GUIDE_VERSION, state });
}

/**
 * 侧栏进度卡要不要显示：没完成也没关掉过、状态都读到了，且还有没做完的步骤。
 * `sawIncomplete`：这次打开客户端时见过前三步没做完。见过才接着提示「启动 Codex」；
 * 一打开就全配好的老用户不显示。
 */
export function guideCardVisible(record: GuideRecord | null, progress: GuideProgress, sawIncomplete: boolean): boolean {
  if (record !== null || progress.loading) return false;
  return !progress.setupDone || (sawIncomplete && !progress.done.launch);
}
