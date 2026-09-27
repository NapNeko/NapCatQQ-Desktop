// 最近命令，按开终端的目标各记一份，存 localStorage；可以清空。

import { useSyncExternalStore } from 'react';
import { createStore } from '../utils/createStore';
import { pushRecent, shouldRemember } from '../../core/domain/terminal/commands';

const KEY = 'ncd.terminal.recents.v1';

type Recents = Record<string, string[]>;

function load(): Recents {
    try {
        const raw = window.localStorage.getItem(KEY);
        const parsed = raw ? (JSON.parse(raw) as unknown) : {};
        return parsed && typeof parsed === 'object' ? (parsed as Recents) : {};
    } catch {
        return {};
    }
}

const store = createStore<Recents>(load());

function persist(next: Recents) {
    store.setState(next);
    try {
        window.localStorage.setItem(KEY, JSON.stringify(next));
    } catch {
        // 存不了就只在这次会话里有
    }
}

const EMPTY: string[] = [];

export const terminalRecents = {
    remember(target: string, command: string) {
        if (!shouldRemember(command)) return;
        const current = store.getSnapshot();
        persist({ ...current, [target]: pushRecent(current[target] ?? EMPTY, command) });
    },
    clear(target: string) {
        const { [target]: _dropped, ...rest } = store.getSnapshot();
        persist(rest);
    },
    get(target: string): string[] {
        return store.getSnapshot()[target] ?? EMPTY;
    },
};

export function useTerminalRecents(target: string): string[] {
    return useSyncExternalStore(
        store.subscribe,
        () => store.getSnapshot()[target] ?? EMPTY,
        () => EMPTY,
    );
}
