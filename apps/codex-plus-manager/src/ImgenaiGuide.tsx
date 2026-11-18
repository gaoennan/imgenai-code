/**
 * imgenai 上手指引（imgenai Code 分发版）：替代上游教人「添加供应商」的入门清单与界面导览。
 *
 * - 欢迎卡：每次打开客户端都弹（没配好的带着从头做，配好的告诉他怎么用），
 *   勾上「以后不再弹出」才不再自动弹；侧栏「上手指引」和「设置 → 通用」随时能再打开。
 * - 侧栏进度卡 `ImgenaiGuideCard`：还没配好时一直在，写着下一步，点「继续」打开指引面板。
 * - 指引面板 `ImgenaiGuidePanel`：停在右下角，当前这一步要点的按钮在界面上圈出来；
 *   每一步按真实状态自动打勾、往下走（见 imgenai-guide-model.ts）。
 *
 * 文案自带中英两份（`L`），不改上游的翻译词典，合并上游时少冲突。
 */
import { useCallback, useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { invoke } from "@tauri-apps/api/core";
import { ArrowLeft, ArrowRight, Check, ChevronDown, ChevronUp, Loader2, Play, RotateCcw, Sparkles, X } from "lucide-react";
import { getLanguage } from "./i18n";
import type { AgentFocus } from "./agent-focus-model";
import { useGatewayPayload } from "./HubOverview";
import { publishGatewayPayload } from "./gateway-store";
import type { Notify } from "./gateway-ui";
import type { GatewayPayload, GatewayResult } from "./gateway-model";
import {
  GUIDE_STORAGE_KEY,
  guideCardVisible,
  guideProgress,
  guideSequence,
  IMGENAI_ACCOUNT_KEY,
  imgenaiCodexBinding,
  imgenaiEntry,
  MAIN_STEPS,
  neighborStep,
  parseGuideRecord,
  recommendedModel,
  serializeGuideRecord,
  type GuideProgress,
  type GuideRecord,
  type GuideStepId,
  type MainStep,
} from "./imgenai-guide-model";
import "./ImgenaiGuide.css";

const L = (zh: string, en: string) => (getLanguage() === "en" ? en : zh);
/** 欢迎卡上勾了「以后不再弹出」。没勾时每次打开客户端都弹。 */
const NO_AUTO_OPEN_KEY = "imgenai.guide.noAutoOpen";
const CODEX_SITE = "https://chatgpt.com/codex";

export function readGuideRecord(): GuideRecord | null {
  try { return parseGuideRecord(window.localStorage.getItem(GUIDE_STORAGE_KEY)); } catch { return null; }
}

export function writeGuideRecord(state: GuideRecord) {
  try { window.localStorage.setItem(GUIDE_STORAGE_KEY, serializeGuideRecord(state)); } catch { /* 存不了只影响这次 */ }
}

/** 侧栏入口等处的名字。 */
export const guideLabel = () => L("上手指引", "Getting started");

/** 打开客户端时要不要自动弹欢迎卡：用户在欢迎卡上勾了「以后不再弹出」就不弹。读不到存储时按要弹处理。 */
export function readNoAutoOpen(): boolean {
  try { return window.localStorage.getItem(NO_AUTO_OPEN_KEY) === "1"; } catch { return false; }
}

function writeNoAutoOpen(off: boolean) {
  try {
    if (off) window.localStorage.setItem(NO_AUTO_OPEN_KEY, "1");
    else window.localStorage.removeItem(NO_AUTO_OPEN_KEY);
  } catch { /* 同上 */ }
}

type AccountState = { ok?: boolean; loggedIn?: boolean };

function callAccount<T>(method: string, params: Record<string, unknown> = {}) {
  return invoke<T>("extension_manager_backend_call", { key: IMGENAI_ACCOUNT_KEY, method, params });
}

/** imgenai 账户是否已登录。拓展停用、调用失败都按没登录处理（指引会带用户去账户页）。null：还没读到。 */
function useAccountLoggedIn(intervalMs: number): boolean | null {
  const [loggedIn, setLoggedIn] = useState<boolean | null>(null);
  useEffect(() => {
    let cancelled = false;
    const load = () => {
      if (document.visibilityState === "hidden") return;
      callAccount<AccountState>("state")
        .then((result) => { if (!cancelled) setLoggedIn(result?.ok === true && result.loggedIn === true); })
        .catch(() => { if (!cancelled) setLoggedIn(false); });
    };
    load();
    const timer = window.setInterval(load, intervalMs);
    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [intervalMs]);
  return loggedIn;
}

export type GuideEnvironment = {
  focus: AgentFocus;
  codexInstalled: boolean | null;
  codexRunning: boolean;
};

/** `fast`：指引面板开着，用户正跟着做，状态变了要马上打勾；侧栏进度卡慢一点就够。 */
function useGuide(env: GuideEnvironment, fast: boolean): { progress: GuideProgress; payload: GatewayPayload | null } {
  const payload = useGatewayPayload(fast ? 3000 : 8000);
  const loggedIn = useAccountLoggedIn(fast ? 2500 : 8000);
  return { progress: guideProgress({ ...env, loggedIn, payload }), payload };
}

const STEP_TITLES: Record<GuideStepId, () => string> = {
  mode: () => L("切换到「只管理 Codex」", "Switch to \"Codex only\""),
  install: () => L("先安装 Codex 桌面版", "Install the Codex desktop app first"),
  login: () => L("登录 imgenai 账户", "Sign in to imgenai"),
  subscribe: () => L("开通订阅或免费试用", "Subscribe or start a free trial"),
  connect: () => L("让 Codex 用上 imgenai", "Connect Codex to imgenai"),
  launch: () => L("启动 Codex", "Launch Codex"),
  done: () => L("全部完成", "All set"),
};

/** 侧栏底部的进度卡：还没配好时一直在，写着下一步。 */
export function ImgenaiGuideCard({ env, hidden, onOpen, onRecord }: {
  env: GuideEnvironment;
  /** 指引面板开着时不重复显示，但仍挂着（要记录完成）。 */
  hidden: boolean;
  onOpen: () => void;
  onRecord: (state: GuideRecord) => void;
}) {
  const { progress } = useGuide(env, false);
  const sawIncomplete = useRef(false);
  if (!progress.loading && !progress.setupDone) sawIncomplete.current = true;

  useEffect(() => {
    if (progress.loading) return;
    // 一打开就全配好了（老用户），或这次跟着做完了：记成完成，卡片收起、不再轮询。
    if (progress.setupDone && (!sawIncomplete.current || progress.done.launch)) onRecord("completed");
  }, [progress, onRecord]);

  if (hidden || !guideCardVisible(null, progress, sawIncomplete.current)) return null;
  const next = STEP_TITLES[progress.current]();
  return (
    <section aria-label={L("上手指引", "Getting started")} className="imgenai-guide-card">
      <header>
        <strong><Sparkles aria-hidden="true" size={13} />{L("上手指引", "Getting started")}</strong>
        <span className="imgenai-guide-count">{`${progress.count}/4`}</span>
        <button aria-label={L("不再显示", "Don't show again")} className="imgenai-guide-icon" onClick={() => onRecord("dismissed")} title={L("不再显示（侧栏「上手指引」可以重新打开）", "Hide (reopen from Getting started in the sidebar)")} type="button">
          <X aria-hidden="true" size={13} />
        </button>
      </header>
      <div className="imgenai-guide-bar" aria-hidden="true"><i style={{ width: `${(progress.count / 4) * 100}%` }} /></div>
      <p>{L("下一步：", "Next: ")}{next}</p>
      <button className="imgenai-guide-primary" onClick={onOpen} type="button">
        {L("继续", "Continue")}<ArrowRight aria-hidden="true" size={13} />
      </button>
    </section>
  );
}

export type GuideLocation = { route: string; extensionKey: string | null };

export type GuideActions = {
  notify: Notify;
  openAccount: () => void;
  openGateway: () => void;
  openCodexSettings: () => void;
  switchToCodexMode: () => void;
  launch: () => Promise<boolean>;
  restart: () => Promise<boolean>;
  launchPending: boolean;
};

/** 圈出界面上要点的元素：不挡点击，跟着窗口和滚动走。 */
function Highlight({ selector, label }: { selector: string | null; label: string }) {
  const [rect, setRect] = useState<DOMRect | null>(null);
  useLayoutEffect(() => {
    if (!selector) {
      setRect(null);
      return;
    }
    const first = document.querySelector<HTMLElement>(selector);
    first?.scrollIntoView({ block: "nearest", behavior: "smooth" });
    const measure = () => {
      const element = document.querySelector<HTMLElement>(selector);
      const box = element?.getBoundingClientRect();
      setRect((current) => {
        if (!box || box.width === 0 || box.height === 0) return null;
        if (current && current.left === box.left && current.top === box.top && current.width === box.width && current.height === box.height) return current;
        return box;
      });
    };
    measure();
    // 页面切换、列表加载完、侧栏展开都会挪位置：轻量轮询比监听每一种变化可靠。
    const timer = window.setInterval(measure, 400);
    window.addEventListener("resize", measure);
    document.addEventListener("scroll", measure, true);
    return () => {
      window.clearInterval(timer);
      window.removeEventListener("resize", measure);
      document.removeEventListener("scroll", measure, true);
    };
  }, [selector]);
  if (!rect) return null;
  const pad = 4;
  // 侧栏里的目标标签放右边（右边是页面空白处）；页面里的目标放下方，免得挡住同一行别的按钮。
  const labelLeft = rect.right + 12;
  const roomRight = rect.right < 320 && labelLeft + 90 < window.innerWidth;
  return createPortal(
    <>
      <div className="imgenai-guide-ring" style={{ left: rect.left - pad, top: rect.top - pad, width: rect.width + pad * 2, height: rect.height + pad * 2 }} />
      <div
        className="imgenai-guide-pointer"
        data-side={roomRight ? "right" : "below"}
        style={roomRight
          ? { left: labelLeft, top: rect.top + rect.height / 2 }
          : { left: Math.max(8, Math.min(rect.left, window.innerWidth - 120)), top: rect.bottom + 12 }}
      >
        {label}
      </div>
    </>,
    document.body,
  );
}

type StepView = { body: ReactNode; target: string | null; actions: ReactNode };

export function ImgenaiGuidePanel({ env, location, actions, welcome, onStart, onClose, onFinish }: {
  env: GuideEnvironment;
  location: GuideLocation;
  actions: GuideActions;
  /** 居中的欢迎卡（第一次自动弹出、或从设置里重新打开时）。 */
  welcome: boolean;
  onStart: () => void;
  onClose: () => void;
  onFinish: () => void;
}) {
  const { progress, payload } = useGuide(env, true);
  const [collapsed, setCollapsed] = useState(false);
  const [busy, setBusy] = useState(false);
  const [manualConnect, setManualConnect] = useState(false);
  // 欢迎卡上的「以后不再弹出」：关掉或开始时才存，手动重新打开时显示当前设置，取消勾选就恢复每次弹出。
  const [noAutoOpen, setNoAutoOpen] = useState(readNoAutoOpen);
  // 用户点开或翻到的那一页；null 时跟着当前该做的一步走（做完自动往下）。
  const [viewing, setViewing] = useState<GuideStepId | null>(null);
  const step = progress.current;
  const shown = viewing ?? step;
  const onAccount = location.route === "extension" && location.extensionKey === IMGENAI_ACCOUNT_KEY;
  const onGateway = location.route === "gateway";
  const entry = imgenaiEntry(payload);
  const model = recommendedModel(entry);

  // 换到下一步时自动展开，免得用户在收起状态下不知道已经往下走了。
  const previous = useRef(step);
  useEffect(() => {
    if (previous.current !== step) {
      previous.current = step;
      setCollapsed(false);
      setManualConnect(false);
    }
  }, [step]);
  // 翻到某一页；翻到当前该做的那一步时恢复「跟着走」。
  const go = (id: GuideStepId) => {
    setViewing(id === step ? null : id);
    setManualConnect(false);
    setCollapsed(false);
  };

  const connect = useCallback(async () => {
    if (busy || !payload) return;
    const plan = imgenaiCodexBinding(payload);
    if ("error" in plan) {
      if (plan.manual) {
        setManualConnect(true);
        actions.openGateway();
      }
      await actions.notify(L("还不能接入", "Can't connect yet"), plan.error, "warning");
      return;
    }
    setBusy(true);
    try {
      const drift = Boolean(payload.agents.find((agent) => agent.agent === "codex")?.drift);
      const result = await invoke<GatewayResult>("save_gateway_binding", { agent: "codex", binding: plan.binding, reconnect: drift });
      publishGatewayPayload(result);
      if (result.status !== "ok") await actions.notify(L("接入失败", "Couldn't connect"), result.message, "failed");
    } catch (error) {
      await actions.notify(L("接入失败", "Couldn't connect"), String(error), "failed");
    } finally {
      setBusy(false);
    }
  }, [actions, busy, payload]);

  const views: Record<GuideStepId, () => StepView> = {
    mode: () => ({
      body: L("现在是「只用增强功能」模式，账户页和网关都被隐藏了。imgenai 要经网关接入 Codex，先切到「只管理 Codex」。",
        "You're in \"enhancements only\" mode, which hides the account page and the gateway. imgenai reaches Codex through the gateway, so switch to \"Codex only\" first."),
      target: null,
      actions: <button className="imgenai-guide-primary" onClick={actions.switchToCodexMode} type="button">{L("切换", "Switch")}</button>,
    }),
    install: () => ({
      body: L("本机没找到 Codex。先到 OpenAI 官网下载安装 Codex 桌面版，装好后回到这里，这一步会自动打勾。装在别的位置的话，在 Codex 设置里指定路径。",
        "Codex wasn't found on this computer. Download and install the Codex desktop app from OpenAI, then come back here. If it's installed somewhere else, set its path in Codex settings."),
      target: null,
      actions: (
        <>
          <button className="imgenai-guide-primary" onClick={() => void callAccount("openUrl", { url: CODEX_SITE }).catch(() => undefined)} type="button">{L("打开 Codex 官网", "Open the Codex site")}</button>
          <button className="imgenai-guide-ghost" onClick={actions.openCodexSettings} type="button">{L("Codex 设置", "Codex settings")}</button>
        </>
      ),
    }),
    login: () => onAccount ? {
      body: L("在这一页输入 imgenai.net 的手机号和密码，点「登录」。还没有账号的话，点「注册账号」到官网注册，注册好回来登录。",
        "Enter the phone number and password of your imgenai.net account and click Sign in. No account yet? Click Register, sign up on the website, then come back."),
      target: null,
      actions: null,
    } : {
      body: L("点左侧圈出来的「imgenai 账户」。", "Click \"imgenai 账户\" (imgenai account) highlighted on the left."),
      target: `[data-extension-page="${IMGENAI_ACCOUNT_KEY}"]`,
      actions: <button className="imgenai-guide-primary" onClick={actions.openAccount} type="button">{L("带我去", "Take me there")}<ArrowRight aria-hidden="true" size={13} /></button>,
    },
    subscribe: () => onAccount ? {
      body: L("在账户页选一个档位扫码支付，或点「免费试用」。开通后这一步会自动打勾。",
        "On the account page, pick a plan and pay by QR code, or start the free trial. This step checks itself off once it's active."),
      target: null,
      actions: null,
    } : {
      body: L("点左侧圈出来的「imgenai 账户」，在账户页开通订阅或免费试用。", "Click \"imgenai 账户\" on the left and subscribe or start a free trial there."),
      target: `[data-extension-page="${IMGENAI_ACCOUNT_KEY}"]`,
      actions: <button className="imgenai-guide-primary" onClick={actions.openAccount} type="button">{L("带我去", "Take me there")}<ArrowRight aria-hidden="true" size={13} /></button>,
    },
    connect: () => manualConnect || onGateway ? {
      body: L("点 Codex 这一行圈出来的「默认模型」按钮，选 imgenai 下面的任意模型，选完就接入了。也可以直接点「一键接入」。",
        "Click the highlighted model button on the Codex row and pick any model under imgenai; that connects it. Or just click Connect for me."),
      target: onGateway ? '[data-agent-row="codex"] .gw-picker-button' : '[data-nav="gateway"]',
      actions: (
        <>
          <button className="imgenai-guide-primary" disabled={busy} onClick={() => void connect()} type="button">{busy ? <Loader2 aria-hidden="true" className="animate-spin" size={13} /> : null}{L("一键接入", "Connect for me")}</button>
          {onGateway ? null : <button className="imgenai-guide-ghost" onClick={actions.openGateway} type="button">{L("打开 Codex 接入", "Open Codex setup")}</button>}
        </>
      ),
    } : {
      body: model
        ? L(`一键把 Codex 接到 imgenai，默认用 ${model}。之后在 Codex 对话框下方随时能换别的 imgenai 模型。`,
          `Connect Codex to imgenai in one click, with ${model} as the default. You can switch to any other imgenai model in Codex later.`)
        : L("一键把 Codex 接到 imgenai。", "Connect Codex to imgenai in one click."),
      target: null,
      actions: (
        <>
          <button className="imgenai-guide-primary" disabled={busy} onClick={() => void connect()} type="button">{busy ? <Loader2 aria-hidden="true" className="animate-spin" size={13} /> : null}{L("一键接入", "Connect for me")}</button>
          <button className="imgenai-guide-ghost" onClick={() => { setManualConnect(true); actions.openGateway(); }} type="button">{L("我自己选模型", "I'll pick a model")}</button>
        </>
      ),
    },
    launch: () => ({
      body: L("点「启动 Codex」，或点左侧 Codex 那一行圈出来的播放键。Codex 打开后新建对话，就是在用 imgenai 了。",
        "Click Launch Codex, or the highlighted play button on the Codex row at the left. Start a new chat in Codex and you're using imgenai."),
      target: '[data-guide="codex-run"]',
      actions: (
        <button className="imgenai-guide-primary" disabled={actions.launchPending} onClick={() => void actions.launch()} type="button">
          {actions.launchPending ? <Loader2 aria-hidden="true" className="animate-spin" size={13} /> : <Play aria-hidden="true" size={13} />}
          {actions.launchPending ? L("正在启动…", "Launching…") : L("启动 Codex", "Launch Codex")}
        </button>
      ),
    }),
    done: () => ({
      body: L("在 Codex 里新建对话就会用 imgenai 的模型，对话框下方可以换模型。额度和充值在「imgenai 账户」，每次花了多少在「用量」。Codex 之前就开着、新对话里看不到 imgenai 的模型的话，重启一次 Codex。",
        "New chats in Codex now use imgenai models; switch models below the chat box. Quota and top-ups are on the imgenai account page, and spending is on the Usage page. If Codex was already open and you don't see imgenai models in a new chat, restart Codex once."),
      target: null,
      actions: (
        <>
          <button className="imgenai-guide-primary" onClick={onFinish} type="button"><Check aria-hidden="true" size={13} />{L("完成", "Done")}</button>
          <button className="imgenai-guide-ghost" disabled={actions.launchPending} onClick={() => void actions.restart()} type="button"><RotateCcw aria-hidden="true" size={13} />{L("重启 Codex", "Restart Codex")}</button>
        </>
      ),
    }),
  };

  // 已经做完的步骤：回看时讲清在哪做、以后在哪改，照样圈出位置。
  const accountTarget = onAccount ? null : `[data-extension-page="${IMGENAI_ACCOUNT_KEY}"]`;
  const goAccount = onAccount ? null : <button className="imgenai-guide-ghost" onClick={actions.openAccount} type="button">{L("带我去", "Take me there")}<ArrowRight aria-hidden="true" size={13} /></button>;
  const doneViews: Record<MainStep, () => StepView> = {
    login: () => ({
      body: L("这一步已完成。登录、添加或切换账号都在左侧圈出来的「imgenai 账户」里，用 imgenai.net 的手机号和密码登录。",
        "Done. Sign in, add or switch accounts on \"imgenai 账户\" (highlighted on the left) with your imgenai.net phone number and password."),
      target: accountTarget,
      actions: goAccount,
    }),
    subscribe: () => ({
      body: L("这一步已完成。订阅档位、今日额度、加油包、重置卡和免费试用都在「imgenai 账户」页。",
        "Done. Plans, today's quota, boost packs, reset cards and the free trial are all on the imgenai account page."),
      target: accountTarget,
      actions: goAccount,
    }),
    connect: () => ({
      body: L("这一步已完成。想换 Codex 的默认模型，到「Codex 接入」点 Codex 这一行圈出来的「默认模型」按钮，选 imgenai 下的模型。",
        "Done. To change Codex's default model, open Codex setup and click the highlighted default model button on the Codex row, then pick a model under imgenai."),
      target: onGateway ? '[data-agent-row="codex"] .gw-picker-button' : '[data-nav="gateway"]',
      actions: onGateway ? null : <button className="imgenai-guide-ghost" onClick={actions.openGateway} type="button">{L("带我去", "Take me there")}<ArrowRight aria-hidden="true" size={13} /></button>,
    }),
    launch: () => ({
      body: L("这一步已完成，Codex 正在运行。以后用左侧 Codex 那一行圈出来的按钮启动或停止 Codex。",
        "Done, Codex is running. Use the highlighted button on the Codex row at the left to start or stop Codex."),
      target: '[data-guide="codex-run"]',
      actions: null,
    }),
  };

  // 欢迎卡点「开始」：当前这一步在账户页做的，顺手打开账户页。已经配好的从第一步看起。
  const begin = () => {
    writeNoAutoOpen(noAutoOpen);
    if (progress.setupDone) setViewing("login");
    else if ((step === "login" || step === "subscribe") && !onAccount) actions.openAccount();
    onStart();
  };
  const later = () => {
    writeNoAutoOpen(noAutoOpen);
    onClose();
  };

  if (welcome) {
    // 状态读到再显示，免得勾先空着再一下子全打上。
    if (progress.loading) return null;
    const ready = progress.setupDone;
    return createPortal(
      <div className="imgenai-guide-backdrop">
        <div aria-labelledby="imgenai-guide-welcome-title" aria-modal="true" className="imgenai-guide-welcome" role="dialog">
          <h3 id="imgenai-guide-welcome-title">{L("欢迎使用 imgenai Code", "Welcome to imgenai Code")}</h3>
          <p>{ready
            ? L("这台电脑已经登录 imgenai、接好了 Codex，打开 Codex 新建对话就能用。点「看看怎么用」从第一步看起：在哪登录和开通、怎么接入和启动 Codex、在哪换模型和看额度。",
              "This computer is signed in to imgenai and Codex is connected; start a new chat in Codex to use it. Click How to use it to go through each step: where to sign in and subscribe, how Codex is connected and launched, and where to switch models and check quota.")
            : L("跟着下面几步，几分钟就能在 Codex 里用上 imgenai 的模型。每一步做完会自动打勾。", "Follow these steps to start using imgenai models in Codex. Each step checks itself off when it's done.")}</p>
          <ol className="imgenai-guide-steps">
            {MAIN_STEPS.map((id, index) => (
              <li data-done={progress.done[id]} key={id}>
                <span className="imgenai-guide-num">{progress.done[id] ? <Check aria-hidden="true" size={12} /> : index + 1}</span>
                {STEP_TITLES[id]()}
              </li>
            ))}
          </ol>
          <div className="imgenai-guide-actions">
            <label className="imgenai-guide-optout">
              <input checked={noAutoOpen} onChange={(event) => setNoAutoOpen(event.target.checked)} type="checkbox" />
              {L("以后不再弹出", "Don't show on startup")}
            </label>
            <button className="imgenai-guide-ghost" onClick={later} type="button">{L("以后再说", "Later")}</button>
            <button autoFocus className="imgenai-guide-primary" onClick={begin} type="button">{ready ? L("看看怎么用", "How to use it") : L("开始", "Start")}<ArrowRight aria-hidden="true" size={13} /></button>
          </div>
        </div>
      </div>,
      document.body,
    );
  }

  const mainShown = (MAIN_STEPS as readonly GuideStepId[]).includes(shown) ? (shown as MainStep) : null;
  const view = mainShown && progress.done[mainShown] ? doneViews[mainShown]() : views[shown]();
  const blockingShown = shown === "mode" || shown === "install";
  const sequence = guideSequence(progress.setupDone);
  const prev = neighborStep(sequence, shown, -1);
  const next = neighborStep(sequence, shown, 1);
  return createPortal(
    <>
      {!collapsed && view.target ? <Highlight label={L("点这里", "Click here")} selector={view.target} /> : null}
      <aside aria-label={L("上手指引", "Getting started")} className="imgenai-guide-panel" data-collapsed={collapsed}>
        <header>
          <strong><Sparkles aria-hidden="true" size={13} />{progress.current === "done" ? L("上手指引 · 已配好", "Getting started · all set") : L("上手指引", "Getting started")}</strong>
          <span className="imgenai-guide-count">{`${progress.count}/4`}</span>
          <button aria-label={collapsed ? L("展开", "Expand") : L("收起", "Collapse")} className="imgenai-guide-icon" onClick={() => setCollapsed(!collapsed)} type="button">
            {collapsed ? <ChevronUp aria-hidden="true" size={14} /> : <ChevronDown aria-hidden="true" size={14} />}
          </button>
          <button aria-label={L("关闭指引", "Close guide")} className="imgenai-guide-icon" onClick={onClose} title={L("关闭（侧栏「上手指引」可以重新打开）", "Close (reopen from Getting started in the sidebar)")} type="button">
            <X aria-hidden="true" size={14} />
          </button>
        </header>
        {collapsed ? (
          <button className="imgenai-guide-collapsed" onClick={() => setCollapsed(false)} type="button">{STEP_TITLES[shown]()}</button>
        ) : (
          <>
            {blockingShown ? (
              <div className="imgenai-guide-current" data-blocking="true">
                <strong>{STEP_TITLES[shown]()}</strong>
                <p>{view.body}</p>
                {view.actions ? <div className="imgenai-guide-actions">{view.actions}</div> : null}
              </div>
            ) : null}
            {/* 四步都列着，点哪一步看哪一步（做完的也能回看）。 */}
            <ol className="imgenai-guide-steps">
              {MAIN_STEPS.map((id, index) => {
                const done = progress.done[id];
                const active = id === shown;
                return (
                  <li data-active={active} data-done={done} key={id}>
                    <span className="imgenai-guide-num">{done ? <Check aria-hidden="true" size={12} /> : index + 1}</span>
                    <div>
                      <button aria-expanded={active} className="imgenai-guide-step-title" onClick={() => go(id)} type="button">{STEP_TITLES[id]()}</button>
                      {active ? (
                        <div className="imgenai-guide-current">
                          <p>{view.body}</p>
                          {view.actions ? <div className="imgenai-guide-actions">{view.actions}</div> : null}
                        </div>
                      ) : null}
                    </div>
                  </li>
                );
              })}
            </ol>
            {shown === "done" ? (
              <div className="imgenai-guide-current imgenai-guide-summary">
                <strong>{L("怎么用", "How to use it")}</strong>
                <p>{view.body}</p>
                <div className="imgenai-guide-actions">{view.actions}</div>
              </div>
            ) : null}
            <nav aria-label={L("翻页", "Pages")} className="imgenai-guide-nav">
              <button className="imgenai-guide-ghost" disabled={!prev} onClick={() => prev && go(prev)} type="button"><ArrowLeft aria-hidden="true" size={13} />{L("上一步", "Back")}</button>
              <button className="imgenai-guide-ghost" disabled={!next} onClick={() => next && go(next)} type="button">{next === "done" ? L("怎么用", "How to use it") : L("下一步", "Next")}<ArrowRight aria-hidden="true" size={13} /></button>
            </nav>
          </>
        )}
      </aside>
    </>,
    document.body,
  );
}
