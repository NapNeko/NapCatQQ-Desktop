// 终端会话表和面板状态：开着哪些会话、分成几个标签（一个标签里可以左右 / 上下分两块）、
// 面板开没开、最大化没有。会话本身活在后端；网页重建（轻量模式、刷新）后 bootstrap 拉一遍列表，
// 标签布局从 localStorage 找回来，对不上的会话各自成一个标签。

import { useSyncExternalStore } from 'react';
import { createStore } from '../utils/createStore';
import { terminalService } from '../../core/services/terminal.service';
import { targetKey } from '../../core/domain/terminal/commands';
import { errorText } from '../../core/domain/errors';
import { pushErrorBar } from '../ui/pushErrorBar';
import { terminalLayout, terminalPrefs } from './terminalPrefs';
import type { LocalShellKind } from '../../core/ipc/generated/domain/LocalShellKind';
import type { TerminalSessionInfo } from '../../core/ipc/generated/domain/TerminalSessionInfo';
import type { TerminalStatus } from '../../core/ipc/generated/domain/TerminalStatus';
import type { TerminalTarget } from '../../core/ipc/generated/domain/TerminalTarget';

export type TerminalActivity = 'none' | 'output' | 'ok' | 'fail';
export type TerminalSplit = 'row' | 'column';

export interface TerminalSessionView {
    info: TerminalSessionInfo;
    /** shell 报上来的当前目录；没有命令标记时是开的时候的目录 */
    cwd: string | null;
    /** 不在眼前时来了输出 / 跑完了命令 */
    activity: TerminalActivity;
    progress: { state: number; value: number } | null;
    customTitle: string | null;
    /** sudo 在等密码 */
    sudoPrompt: boolean;
    /** 这一块的文件栏开没开。各块各管各的：新开的跟着上回的习惯，分屏分出来的那块先收着，不然两边都挤 */
    filesOpen: boolean;
}

export interface TerminalGroup {
    id: string;
    panes: string[];
    split: TerminalSplit;
    /** 第一块占的比例 */
    ratio: number;
    focused: string;
}

export interface TerminalState {
    ready: boolean;
    sessions: Record<string, TerminalSessionView>;
    groups: TerminalGroup[];
    activeGroup: string | null;
    open: boolean;
    maximized: boolean;
    busy: boolean;
}

const GROUPS_KEY = 'ncd.terminal.groups.v1';

const store = createStore<TerminalState>({
    ready: false,
    sessions: {},
    groups: [],
    activeGroup: null,
    open: false,
    maximized: false,
    busy: false,
});

const get = store.getSnapshot;
const set = (patch: Partial<TerminalState>) => store.setState({ ...get(), ...patch });

let nextGroup = 1;
const newGroupId = () => `g${Date.now().toString(36)}${nextGroup++}`;

function persistGroups(groups: TerminalGroup[], activeGroup: string | null) {
    try {
        window.localStorage.setItem(GROUPS_KEY, JSON.stringify({ groups, activeGroup }));
    } catch {
        // 存不了就下次重建时每个会话各一个标签
    }
}

function setGroups(groups: TerminalGroup[], activeGroup: string | null, extra: Partial<TerminalState> = {}) {
    set({ groups, activeGroup, ...extra });
    persistGroups(groups, activeGroup);
}

function viewOf(info: TerminalSessionInfo, filesOpen: boolean): TerminalSessionView {
    return {
        info,
        cwd: info.cwd ?? null,
        activity: 'none',
        progress: null,
        customTitle: null,
        sudoPrompt: false,
        filesOpen,
    };
}

function patchSession(id: string, patch: Partial<TerminalSessionView>) {
    const current = get().sessions[id];
    if (!current) return;
    set({ sessions: { ...get().sessions, [id]: { ...current, ...patch } } });
}

function groupOf(sessionId: string): TerminalGroup | undefined {
    return get().groups.find((g) => g.panes.includes(sessionId));
}

/** 标签切过去时，这个标签里的会话的活动提示就算看过了 */
function clearActivityOf(group: TerminalGroup | undefined) {
    if (!group) return;
    const sessions = { ...get().sessions };
    let changed = false;
    for (const id of group.panes) {
        const s = sessions[id];
        if (s && s.activity !== 'none') {
            sessions[id] = { ...s, activity: 'none' };
            changed = true;
        }
    }
    if (changed) set({ sessions });
}

function restoreGroups(ids: string[]): { groups: TerminalGroup[]; activeGroup: string | null } {
    let saved: { groups?: TerminalGroup[]; activeGroup?: string | null } = {};
    try {
        saved = JSON.parse(window.localStorage.getItem(GROUPS_KEY) ?? '{}');
    } catch {
        saved = {};
    }
    const alive = new Set(ids);
    const used = new Set<string>();
    const groups: TerminalGroup[] = [];
    for (const g of saved.groups ?? []) {
        const panes = (g.panes ?? []).filter((p) => alive.has(p) && !used.has(p)).slice(0, 2);
        if (!panes.length) continue;
        panes.forEach((p) => used.add(p));
        groups.push({
            id: g.id || newGroupId(),
            panes,
            split: g.split === 'column' ? 'column' : 'row',
            ratio: typeof g.ratio === 'number' ? Math.min(0.85, Math.max(0.15, g.ratio)) : 0.5,
            focused: panes.includes(g.focused) ? g.focused : (panes[0] as string),
        });
    }
    for (const id of ids) {
        if (!used.has(id)) groups.push({ id: newGroupId(), panes: [id], split: 'row', ratio: 0.5, focused: id });
    }
    const activeGroup =
        groups.find((g) => g.id === saved.activeGroup)?.id ?? groups[groups.length - 1]?.id ?? null;
    return { groups, activeGroup };
}

function sameTarget(a: TerminalSessionInfo, target: TerminalTarget, shell: LocalShellKind | undefined): boolean {
    if (targetKey(a.target) !== targetKey(target)) return false;
    return target.kind !== 'local' || !shell || a.shell === shell;
}

export const terminalStore = {
    getSnapshot: get,
    subscribe: store.subscribe,

    async bootstrap() {
        if (get().ready) return;
        try {
            const list = await terminalService.list();
            const { groups, activeGroup } = restoreGroups(list.map((i) => i.id as string));
            const secondPanes = new Set(groups.flatMap((g) => g.panes.slice(1)));
            const filesOpen = terminalLayout.get().filesOpen;
            const sessions: Record<string, TerminalSessionView> = {};
            for (const info of list) {
                const id = info.id as string;
                sessions[id] = viewOf(info, filesOpen && !secondPanes.has(id));
            }
            set({ ready: true, sessions, groups, activeGroup });
        } catch {
            set({ ready: true });
        }
    },

    /// 开一个终端。同一个目标已经开着且还活着就切过去（forceNew 时总是新开）；
    /// splitFrom 给了就放进那个会话所在的标签里分屏
    async open(
        target: TerminalTarget,
        opts: { shell?: LocalShellKind; forceNew?: boolean; splitFrom?: string; split?: TerminalSplit } = {},
    ): Promise<string | null> {
        const shell = opts.shell ?? (target.kind === 'local' ? terminalPrefs.get().defaultShell ?? undefined : undefined);
        if (!opts.forceNew && !opts.splitFrom) {
            const existing = Object.values(get().sessions).find(
                (s) => sameTarget(s.info, target, opts.shell) && (s.info.status.kind === 'running' || s.info.status.kind === 'starting'),
            );
            if (existing) {
                terminalStore.focusPane(existing.info.id as string);
                return existing.info.id as string;
            }
        }
        set({ busy: true });
        try {
            const info = await terminalService.open({ target, cols: 120, rows: 30, shell });
            const id = info.id as string;
            let groups = get().groups;
            let activeGroup: string;
            const host = opts.splitFrom ? groupOf(opts.splitFrom) : undefined;
            const intoSplit = !!host && host.panes.length < 2;
            if (host && intoSplit) {
                groups = groups.map((g) =>
                    g.id === host.id ? { ...g, panes: [...g.panes, id], split: opts.split ?? 'row', ratio: 0.5, focused: id } : g,
                );
                activeGroup = host.id;
            } else {
                const group: TerminalGroup = { id: newGroupId(), panes: [id], split: 'row', ratio: 0.5, focused: id };
                groups = [...groups, group];
                activeGroup = group.id;
            }
            // 会话和它的标签一次放进去：终端运行时一看到新会话就接输出，
            // 中间要是有「有会话没标签」的一刻，头几个字节会被当成后台输出亮活动点
            const sessions = { ...get().sessions, [id]: viewOf(info, !intoSplit && terminalLayout.get().filesOpen) };
            setGroups(groups, activeGroup, { sessions, open: true, busy: false });
            return id;
        } catch (err) {
            set({ busy: false });
            pushErrorBar({ title: '终端没开起来', raw: errorText(err) });
            return null;
        }
    },

    focusGroup(groupId: string) {
        const group = get().groups.find((g) => g.id === groupId);
        if (!group) return;
        setGroups(get().groups, groupId, { open: true });
        clearActivityOf(group);
    },

    focusPane(sessionId: string) {
        const group = groupOf(sessionId);
        if (!group) return;
        const groups = get().groups.map((g) => (g.id === group.id ? { ...g, focused: sessionId } : g));
        setGroups(groups, group.id, { open: true });
        clearActivityOf(group);
    },

    async close(sessionId: string) {
        const { sessions, groups, activeGroup } = get();
        const { [sessionId]: _closed, ...rest } = sessions;
        const nextGroups = groups
            .map((g) => {
                if (!g.panes.includes(sessionId)) return g;
                const panes = g.panes.filter((p) => p !== sessionId);
                return { ...g, panes, focused: panes.includes(g.focused) ? g.focused : (panes[0] ?? '') };
            })
            .filter((g) => g.panes.length > 0);
        let nextActive = activeGroup;
        if (!nextGroups.some((g) => g.id === activeGroup)) {
            const index = groups.findIndex((g) => g.id === activeGroup);
            nextActive = nextGroups[Math.min(Math.max(index, 0), nextGroups.length - 1)]?.id ?? null;
        }
        set({ sessions: rest });
        setGroups(nextGroups, nextActive, nextGroups.length === 0 ? { open: false, maximized: false } : {});
        try {
            await terminalService.close(sessionId);
        } catch {
            // 后端那头已经没了也无所谓
        }
    },

    async closeGroup(groupId: string) {
        const group = get().groups.find((g) => g.id === groupId);
        if (!group) return;
        for (const id of group.panes) await terminalStore.close(id);
    },

    async closeOthers(groupId: string) {
        for (const g of get().groups.filter((x) => x.id !== groupId)) await terminalStore.closeGroup(g.id);
    },

    async restart(sessionId: string) {
        try {
            const info = await terminalService.restart(sessionId);
            patchSession(sessionId, { info, sudoPrompt: false });
        } catch (err) {
            pushErrorBar({ title: '终端没能重新打开', raw: errorText(err) });
        }
    },

    rename(sessionId: string, title: string | null) {
        patchSession(sessionId, { customTitle: title && title.trim() ? title.trim() : null });
    },

    /// 开 / 收某一块的文件栏，顺手记成以后新开终端时的默认
    setFilesOpen(sessionId: string, filesOpen: boolean) {
        patchSession(sessionId, { filesOpen });
        terminalLayout.patch({ filesOpen });
    },

    setOpen(open: boolean) {
        set({ open, maximized: open ? get().maximized : false });
        if (open) clearActivityOf(get().groups.find((g) => g.id === get().activeGroup));
    },

    toggle() {
        if (!get().open && get().groups.length === 0) {
            void terminalStore.open({ kind: 'local' });
            return;
        }
        terminalStore.setOpen(!get().open);
    },

    setMaximized(maximized: boolean) {
        set({ maximized, open: maximized ? true : get().open });
    },

    setRatio(groupId: string, ratio: number) {
        const groups = get().groups.map((g) =>
            g.id === groupId ? { ...g, ratio: Math.min(0.85, Math.max(0.15, ratio)) } : g,
        );
        setGroups(groups, get().activeGroup);
    },

    setSplit(groupId: string, split: TerminalSplit) {
        const groups = get().groups.map((g) => (g.id === groupId ? { ...g, split } : g));
        setGroups(groups, get().activeGroup);
    },

    moveGroup(from: number, to: number) {
        const groups = [...get().groups];
        const [moved] = groups.splice(from, 1);
        if (!moved) return;
        groups.splice(Math.max(0, Math.min(groups.length, to)), 0, moved);
        setGroups(groups, get().activeGroup);
    },

    /// 这个会话此刻在不在眼前（面板开着、它的标签是当前标签）
    isVisible(sessionId: string): boolean {
        const { open, activeGroup } = get();
        return open && groupOf(sessionId)?.id === activeGroup;
    },

    // 下面这些由终端运行时回报

    updateInfo(sessionId: string, info: TerminalSessionInfo) {
        patchSession(sessionId, { info, cwd: info.cwd ?? get().sessions[sessionId]?.cwd ?? null });
    },

    setStatus(sessionId: string, status: TerminalStatus) {
        const current = get().sessions[sessionId];
        if (!current) return;
        patchSession(sessionId, { info: { ...current.info, status }, sudoPrompt: false });
    },

    setCwd(sessionId: string, cwd: string) {
        if (get().sessions[sessionId]?.cwd !== cwd) patchSession(sessionId, { cwd });
    },

    setActivity(sessionId: string, activity: TerminalActivity) {
        const current = get().sessions[sessionId];
        if (!current || current.activity === activity) return;
        // 跑完了命令的提示比「有新输出」更要紧，不被后者盖掉
        if (activity === 'output' && (current.activity === 'ok' || current.activity === 'fail')) return;
        patchSession(sessionId, { activity });
    },

    setProgress(sessionId: string, progress: TerminalSessionView['progress']) {
        patchSession(sessionId, { progress });
    },

    setSudoPrompt(sessionId: string, sudoPrompt: boolean) {
        if (get().sessions[sessionId]?.sudoPrompt !== sudoPrompt) patchSession(sessionId, { sudoPrompt });
    },
};

export function useTerminalState(): TerminalState {
    return useSyncExternalStore(store.subscribe, get, get);
}

/** 应用根只关心面板是不是最大化盖住了页面；只订阅这一位，别的变化不让根重渲 */
export function useTerminalCoversPage(): boolean {
    const covers = () => get().open && get().maximized;
    return useSyncExternalStore(store.subscribe, covers, covers);
}

export function useTerminalSession(sessionId: string): TerminalSessionView | undefined {
    return useSyncExternalStore(
        store.subscribe,
        () => get().sessions[sessionId],
        () => get().sessions[sessionId],
    );
}

/** 标签上显示的名字 */
export function sessionTitle(view: TerminalSessionView): string {
    return view.customTitle ?? view.info.title;
}

export function isLive(status: TerminalStatus): boolean {
    return status.kind === 'running' || status.kind === 'starting';
}

/** 给别的页面用的入口：Bot 卡片、应用端详情、远端主机卡片 */
export function openTerminal(target: TerminalTarget, opts?: { shell?: LocalShellKind; forceNew?: boolean }) {
    return terminalStore.open(target, opts);
}
