// 调试台工作区（标签页草稿、选中的 Bot、各 Bot 的通道选择、栏宽）的模块级 store。
//
// 路由切走再切回来，标签和草稿必须都在（frontend.md 坑 6），所以不放组件里。
// 落盘的只有 `ws`；`runs`（每个标签正在等的调用和最近一次结果）只活在内存里。
// 改动 500ms 防抖写盘，页面卸载 / 窗口藏起来时立刻写（flushWorkspace）。

import { useSyncExternalStore } from 'react';
import { createStore } from '../utils/createStore';
import { onebotDebugService } from '../../core/services/onebot-debug.service';
import { errorText } from '../../core/domain/errors';
import { newRequestId } from '../../core/domain/debug/ids';
import { EXPLICIT_WIDTH_VERSION, UNSET_COLUMN_WIDTH, upgradeLayoutWidths } from '../../core/domain/debug/workbenchLayout';
import { pushErrorBar } from '../ui/pushErrorBar';
import type { DebugCallResponse } from '../../core/ipc/generated/debug/DebugCallResponse';
import type { DebugChannelChoice } from '../../core/ipc/generated/debug/DebugChannelChoice';
import type { DebugChannelId } from '../../core/ipc/generated/debug/DebugChannelId';
import type { DebugLayout } from '../../core/ipc/generated/debug/DebugLayout';
import type { DebugRequestDraft } from '../../core/ipc/generated/debug/DebugRequestDraft';
import type { DebugStreamProgress } from '../../core/ipc/generated/debug/DebugStreamProgress';
import type { DebugWorkspace } from '../../core/ipc/generated/debug/DebugWorkspace';
import { channelIdKey } from './keys';

export interface TabRun {
    /** 正在等的那次调用；同一标签连发时只认最后一次。流式调用的最新一拍进度挂在 progress 上 */
    inflight?: { requestId: string; startedAt: number; progress?: DebugStreamProgress };
    /** 最近一次拿到的结果；记下 Bot 和动作，视图据此丢掉切了 Bot / 换了动作之后才回来的旧结果 */
    last?: { response: DebugCallResponse; at: number; botId: string; action: string };
}

export interface DebugWorkspaceState {
    loaded: boolean;
    ws: DebugWorkspace;
    runs: Record<string, TabRun>;
}

const MAX_CLOSED_TABS = 10;
const MAX_RECENT_ACTIONS = 20;
const SAVE_DELAY_MS = 500;

export function defaultWorkspace(): DebugWorkspace {
    return {
        version: EXPLICIT_WIDTH_VERSION,
        tabs: [],
        active_tab: null,
        closed_tabs: [],
        selected_bot: null,
        channel_choice: {},
        layout: {
            left_collapsed: false,
            right_collapsed: false,
            left_width: UNSET_COLUMN_WIDTH,
            right_width: UNSET_COLUMN_WIDTH,
            right_view: 'chat',
        },
        recent_actions: [],
    };
}

const store = createStore<DebugWorkspaceState>({ loaded: false, ws: defaultWorkspace(), runs: {} });

// ---------------------------------------------------------------------------
// 写盘
// ---------------------------------------------------------------------------

let saveTimer: ReturnType<typeof setTimeout> | null = null;
let dirty = false;
let saving: Promise<void> | null = null;
/** 连续失败只弹一次条；一次写成功后再失败才会再弹 */
let saveFailureShown = false;

function clearSaveTimer(): void {
    if (saveTimer !== null) {
        clearTimeout(saveTimer);
        saveTimer = null;
    }
}

function markDirty(): void {
    dirty = true;
    clearSaveTimer();
    saveTimer = setTimeout(() => {
        saveTimer = null;
        void flushWorkspace();
    }, SAVE_DELAY_MS);
}

/** 立刻写盘（防抖里还没写的也一起写）。页面卸载、窗口藏起来时调；写盘失败会弹错误条，不会 reject */
export function flushWorkspace(): Promise<void> {
    clearSaveTimer();
    // 已经有一轮在写：它写完会看 dirty 再补一轮，等它就行，避免两次调用交叠乱序
    if (saving) return saving;
    if (!dirty) return Promise.resolve();
    const run: Promise<void> = writeUntilClean().finally(() => {
        if (saving === run) saving = null;
    });
    saving = run;
    return run;
}

async function writeUntilClean(): Promise<void> {
    while (dirty) {
        dirty = false;
        // 启动时读盘失败过：内存里只有默认值加上这之后的改动，直接写会把磁盘上原来的工作区盖掉。
        // 先补读一次，读到了才写（内存里的改动套在读到的工作区上）；还读不到就不写，改动留在内存里，
        // 下次保存时机再试
        if (!diskReadable && !(await retryRead())) {
            dirty = true;
            return;
        }
        try {
            await onebotDebugService.saveWorkspace(store.getSnapshot().ws);
            saveFailureShown = false;
        } catch (err) {
            if (!saveFailureShown) {
                saveFailureShown = true;
                pushErrorBar({
                    key: 'debug-workspace-save',
                    title: '保存调试台工作区失败',
                    raw: errorText(err),
                });
            }
        }
    }
}

function onVisibilityChange(): void {
    if (document.visibilityState === 'hidden') void flushWorkspace();
}

let visibilityBound = false;

function bindVisibility(): void {
    if (visibilityBound || typeof document === 'undefined') return;
    document.addEventListener('visibilitychange', onVisibilityChange);
    visibilityBound = true;
}

// ---------------------------------------------------------------------------
// 状态更新
// ---------------------------------------------------------------------------

type WsOp = (ws: DebugWorkspace) => DebugWorkspace;

/**
 * 还没套在「从磁盘读到的工作区」上的改动。载入完成前发生的（比如从 Bot 卡片跳进来时立刻选中 Bot），
 * 或者读盘失败后在内存里做的，读到之后照原样再套一遍，不丢
 */
let pendingOps: WsOp[] = [];

/** 读盘成功过没有。没成功过之前不往磁盘写 */
let diskReadable = false;

/** 标签打开时的初始文本。文本还是它的标签算「没被动过」，下次打开动作可以直接顶替 */
const pristineText = new Map<string, string>();

function updateWs(op: WsOp): void {
    const s = store.getSnapshot();
    const ws = op(s.ws);
    if (ws === s.ws) return;
    store.setState({ ...s, ws });
    if (!diskReadable) pendingOps.push(op);
    if (s.loaded) markDirty();
}

function mapTab(ws: DebugWorkspace, id: string, fn: (t: DebugRequestDraft) => DebugRequestDraft): DebugWorkspace {
    let changed = false;
    const tabs = ws.tabs.map((t) => {
        if (t.id !== id) return t;
        const next = fn(t);
        if (next !== t) changed = true;
        return next;
    });
    return changed ? { ...ws, tabs } : ws;
}

function dropRun(tabId: string): void {
    const s = store.getSnapshot();
    if (!(tabId in s.runs)) return;
    const runs = { ...s.runs };
    delete runs[tabId];
    store.setState({ ...s, runs });
}

function untouched(tab: DebugRequestDraft): boolean {
    const text = tab.params_text.trim();
    return text === '' || text === '{}' || tab.params_text === pristineText.get(tab.id);
}

function blank(tab: DebugRequestDraft): boolean {
    return tab.action === '' && untouched(tab);
}

function activeTabOf(ws: DebugWorkspace): DebugRequestDraft | undefined {
    return ws.tabs.find((t) => t.id === ws.active_tab);
}

function addTab(ws: DebugWorkspace, tab: DebugRequestDraft): DebugWorkspace {
    return { ...ws, tabs: [...ws.tabs, tab], active_tab: tab.id };
}

/** 落盘的 active_tab 指向已不存在的标签时回退到第一个，避免中栏空着却有标签 */
function normalize(ws: DebugWorkspace): DebugWorkspace {
    if (ws.active_tab !== null && ws.tabs.some((t) => t.id === ws.active_tab)) return ws;
    const fallback = ws.tabs[0]?.id ?? null;
    return fallback === ws.active_tab ? ws : { ...ws, active_tab: fallback };
}

// ---------------------------------------------------------------------------
// 载入
// ---------------------------------------------------------------------------

let loadPromise: Promise<void> | null = null;
let loadEpoch = 0;
/** 读盘失败的错误条只在第一次失败时弹；后面每次保存前的补读失败不再重复打扰 */
let loadFailureShown = false;

/** 读到了：把等着的改动套上去，之后才允许写盘。返回套了几条改动 */
function adoptDisk(loaded: DebugWorkspace): number {
    let ws = normalize(upgradeLayoutWidths(loaded));
    const replay = pendingOps;
    pendingOps = [];
    for (const op of replay) ws = op(ws);
    diskReadable = true;
    store.setState({ ...store.getSnapshot(), loaded: true, ws });
    return replay.length;
}

async function retryRead(): Promise<boolean> {
    const epoch = loadEpoch;
    let disk: DebugWorkspace;
    try {
        disk = await onebotDebugService.workspace();
    } catch {
        // 第一次失败时已经弹过条，这里静默；改动还在内存里，下次保存再试
        return false;
    }
    if (epoch !== loadEpoch) return false;
    adoptDisk(disk);
    return true;
}

/** 只读一次；读失败就用默认值在内存里继续用（不写盘），并弹错误条 */
function load(): Promise<void> {
    if (!loadPromise) {
        bindVisibility();
        const epoch = loadEpoch;
        loadPromise = onebotDebugService.workspace().then(
            (ws) => {
                if (epoch !== loadEpoch) return;
                // 载入前做过的改动现在才落在真正的工作区上，要存一次
                if (adoptDisk(ws) > 0) markDirty();
            },
            (err) => {
                if (epoch !== loadEpoch) return;
                if (!loadFailureShown) {
                    loadFailureShown = true;
                    pushErrorBar({
                        key: 'debug-workspace-load',
                        title: '读取调试台工作区失败，这次的改动暂时只保存在内存里',
                        raw: errorText(err),
                    });
                }
                // 已经在默认值上做过的改动留在 pendingOps 里，等读到磁盘后再套
                store.setState({ ...store.getSnapshot(), loaded: true });
            },
        );
    }
    return loadPromise;
}

/**
 * 弹出窗（独立窗口）改写过盘上的工作区后关掉：摘掉「只读一次」的记号牌、回到未载入，
 * 主窗下次进调试页从盘上重读。由弹出窗销毁事件驱动（lib.rs 的 DEBUG_POPOUT_CLOSED），
 * 此刻主窗的调试页必然没挂着（弹出时已导航走）
 */
export function markWorkspaceStale(): void {
    loadEpoch += 1;
    clearSaveTimer();
    dirty = false;
    pendingOps = [];
    diskReadable = false;
    pristineText.clear();
    loadPromise = null;
    const s = store.getSnapshot();
    if (s.loaded) store.setState({ ...s, loaded: false });
}

// ---------------------------------------------------------------------------
// 标签
// ---------------------------------------------------------------------------

/**
 * 从目录 / 命令面板打开一个动作。默认顶替当前标签（只要它没被用户动过），否则另开一个；
 * `paramsText` 由调用方按动作说明算好（initialParamsText），不给就是 `{}`。返回落在哪个标签上。
 */
function openAction(name: string, opts: { newTab?: boolean; paramsText?: string } = {}): string {
    const text = opts.paramsText ?? '{}';
    const current = activeTabOf(store.getSnapshot().ws);
    const replace = !opts.newTab && !!current && untouched(current);
    const id = replace && current ? current.id : newRequestId();
    const previousAction = replace && current ? current.action : null;

    updateWs((ws) => {
        // 载入前的改动重放时，原来的标签可能已经不在了，那就当新开
        if (replace && ws.tabs.some((t) => t.id === id)) {
            const mapped = mapTab(ws, id, (t) =>
                t.action === name && t.params_text === text ? t : { ...t, action: name, params_text: text },
            );
            return mapped.active_tab === id ? mapped : { ...mapped, active_tab: id };
        }
        return addTab(ws, { id, action: name, params_text: text, timeout_ms: null, channel: null });
    });

    pristineText.set(id, text);
    // 换了动作，上一个动作的结果不该挂在新动作下面
    if (previousAction !== null && previousAction !== name) dropRun(id);
    return id;
}

function newTab(): string {
    const id = newRequestId();
    updateWs((ws) => addTab(ws, { id, action: '', params_text: '{}', timeout_ms: null, channel: null }));
    return id;
}

function closeTab(id: string): void {
    updateWs((ws) => {
        const idx = ws.tabs.findIndex((t) => t.id === id);
        if (idx < 0) return ws;
        const tab = ws.tabs[idx];
        const tabs = ws.tabs.filter((t) => t.id !== id);
        // 关的是当前标签就激活右邻，没有右邻就左邻
        const active = ws.active_tab === id ? (tabs[Math.min(idx, tabs.length - 1)]?.id ?? null) : ws.active_tab;
        // 空白标签没有东西可恢复，不占「最近关闭」的名额
        const closed = blank(tab) ? ws.closed_tabs : [tab, ...ws.closed_tabs].slice(0, MAX_CLOSED_TABS);
        return { ...ws, tabs, active_tab: active, closed_tabs: closed };
    });
    dropRun(id);
    pristineText.delete(id);
}

/** 恢复最近关掉的标签；没有可恢复的返回 null */
function reopenClosed(): string | null {
    const { ws: current } = store.getSnapshot();
    const head = current.closed_tabs[0];
    if (!head) return null;
    const id = current.tabs.some((t) => t.id === head.id) ? newRequestId() : head.id;
    updateWs((ws) => {
        const [tab, ...rest] = ws.closed_tabs;
        if (!tab) return ws;
        return { ...ws, closed_tabs: rest, tabs: [...ws.tabs, { ...tab, id }], active_tab: id };
    });
    return id;
}

function setActive(id: string): void {
    updateWs((ws) => (ws.active_tab === id || !ws.tabs.some((t) => t.id === id) ? ws : { ...ws, active_tab: id }));
}

/** `initial` 表示这是编辑器按动作说明填进去的初始文本，不算用户改过 */
function setParamsText(id: string, text: string, opts: { initial?: boolean } = {}): void {
    updateWs((ws) => mapTab(ws, id, (t) => (t.params_text === text ? t : { ...t, params_text: text })));
    if (opts.initial) pristineText.set(id, text);
}

/**
 * 原地给一个标签换动作，参数留着（给了 `paramsText` 就换成它）。给「参数改过了、但确实要换接口」用：
 * 标签的通道、超时都不动；上一个动作的结果不该挂在新动作下面，丢掉；
 * 留下的参数是用户写的，不再算「没动过」，之后从目录点接口会另开标签，不会把它顶掉。
 */
function setTabAction(id: string, action: string, opts: { paramsText?: string } = {}): void {
    const before = store.getSnapshot().ws.tabs.find((t) => t.id === id);
    if (!before) return;
    updateWs((ws) =>
        mapTab(ws, id, (t) => {
            const text = opts.paramsText ?? t.params_text;
            return t.action === action && t.params_text === text ? t : { ...t, action, params_text: text };
        }),
    );
    if (before.action !== action) {
        pristineText.delete(id);
        dropRun(id);
    }
}

function setTabTimeout(id: string, ms: number | null): void {
    updateWs((ws) => mapTab(ws, id, (t) => (t.timeout_ms === ms ? t : { ...t, timeout_ms: ms })));
}

function setTabChannel(id: string, channel: DebugChannelId | null): void {
    updateWs((ws) =>
        mapTab(ws, id, (t) => {
            const same =
                t.channel === channel ||
                (t.channel !== null && channel !== null && channelIdKey(t.channel) === channelIdKey(channel));
            return same ? t : { ...t, channel };
        }),
    );
}

// ---------------------------------------------------------------------------
// 其它工作区字段
// ---------------------------------------------------------------------------

function selectBot(id: string | null): void {
    updateWs((ws) => (ws.selected_bot === id ? ws : { ...ws, selected_bot: id }));
}

function setChannelChoice(botId: string, choice: DebugChannelChoice): void {
    updateWs((ws) => {
        const old = ws.channel_choice[botId];
        if (
            old &&
            channelIdKey(old.call) === channelIdKey(choice.call) &&
            channelIdKey(old.events) === channelIdKey(choice.events)
        ) {
            return ws;
        }
        return { ...ws, channel_choice: { ...ws.channel_choice, [botId]: choice } };
    });
}

function setLayout(patch: Partial<DebugLayout>): void {
    updateWs((ws) => {
        const keys = Object.keys(patch) as Array<keyof DebugLayout>;
        if (keys.every((k) => patch[k] === undefined || ws.layout[k] === patch[k])) return ws;
        return { ...ws, layout: { ...ws.layout, ...patch } };
    });
}

/** 最近用过的动作，新的在前，去重，留 20 个 */
function pushRecent(name: string): void {
    updateWs((ws) =>
        ws.recent_actions[0] === name
            ? ws
            : { ...ws, recent_actions: [name, ...ws.recent_actions.filter((n) => n !== name)].slice(0, MAX_RECENT_ACTIONS) },
    );
}

// ---------------------------------------------------------------------------
// 每个标签的调用状态（不落盘）
// ---------------------------------------------------------------------------

function getRun(tabId: string): TabRun | undefined {
    return store.getSnapshot().runs[tabId];
}

/** 整条替换；传 null 删掉这个标签的记录 */
function setRun(tabId: string, run: TabRun | null): void {
    if (run === null) {
        dropRun(tabId);
        return;
    }
    const s = store.getSnapshot();
    store.setState({ ...s, runs: { ...s.runs, [tabId]: run } });
}

export const debugWorkspaceStore = {
    getSnapshot: store.getSnapshot,
    subscribe: store.subscribe,
    load,
    openAction,
    newTab,
    closeTab,
    reopenClosed,
    setActive,
    setParamsText,
    setTabAction,
    setTimeout: setTabTimeout,
    setTabChannel,
    selectBot,
    setChannelChoice,
    setLayout,
    pushRecent,
    getRun,
    setRun,

    // 测试 / dev 重置用。
    _reset(): void {
        clearSaveTimer();
        dirty = false;
        saving = null;
        saveFailureShown = false;
        loadPromise = null;
        loadEpoch += 1;
        pendingOps = [];
        diskReadable = false;
        loadFailureShown = false;
        pristineText.clear();
        if (visibilityBound) {
            document.removeEventListener('visibilitychange', onVisibilityChange);
            visibilityBound = false;
        }
        store._reset();
    },
};

// ---------------------------------------------------------------------------
// React
// ---------------------------------------------------------------------------

export function useDebugWorkspace(): DebugWorkspaceState {
    return useSyncExternalStore(store.subscribe, store.getSnapshot, store.getSnapshot);
}

/** selector 必须返回 store 里已有的引用（或原始值），别在里面 new 对象 / 数组，否则每次渲染都当成变了 */
export function useDebugWorkspaceSelector<T>(selector: (s: DebugWorkspaceState) => T): T {
    return useSyncExternalStore(
        store.subscribe,
        () => selector(store.getSnapshot()),
        () => selector(store.getSnapshot()),
    );
}

export const useDebugTabs = (): DebugRequestDraft[] => useDebugWorkspaceSelector((s) => s.ws.tabs);

export const useActiveDebugTab = (): DebugRequestDraft | null =>
    useDebugWorkspaceSelector((s) => activeTabOf(s.ws) ?? null);

export const useTabRun = (tabId: string | null): TabRun | undefined =>
    useDebugWorkspaceSelector((s) => (tabId === null ? undefined : s.runs[tabId]));

export const useDebugLayout = (): DebugLayout => useDebugWorkspaceSelector((s) => s.ws.layout);

export const useSelectedDebugBot = (): string | null => useDebugWorkspaceSelector((s) => s.ws.selected_bot);

export const useDebugChannelChoice = (botId: string | null): DebugChannelChoice | undefined =>
    useDebugWorkspaceSelector((s) => (botId === null ? undefined : s.ws.channel_choice[botId]));
