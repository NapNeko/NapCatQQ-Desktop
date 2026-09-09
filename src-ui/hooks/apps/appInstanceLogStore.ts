// 应用端实例日志：缓冲挂模块级 Map，离开详情 / 切走日志 Tab 不丢。
// 订阅在首个调用方挂一次，之后不卸——否则切到列表或基础 Tab 期间的行会漏掉。

import { useCallback, useEffect, useMemo, useSyncExternalStore } from 'react';
import { createStore } from '../utils/createStore';
import { subscribeDomainEvents } from '../../core/services/domain-event-hub';
import { appFrameworkService } from '../../core/services/app-framework.service';
import {
    appendLine,
    buildHistoryEntries,
    canonicalizeLogEntry,
    type LogEntry,
} from '../../core/domain/events/log-buffer';
import type { DomainEvent } from '../../core/ipc/types';

interface State {
    byId: Record<string, LogEntry[]>;
}

const EMPTY: LogEntry[] = [];

const store = createStore<State>({ byId: {} });

let unsubDomain: (() => void) | null = null;

function appendRaw(instanceId: string, line: string): void {
    const rawLines = line.includes('\n') ? line.split('\n') : [line];
    const current = store.getSnapshot().byId[instanceId] ?? EMPTY;
    let next = current;
    for (const raw of rawLines) {
        next = appendLine(next, raw);
    }
    if (next === current) return;
    store.setState({
        byId: { ...store.getSnapshot().byId, [instanceId]: next },
    });
}

function onDomainEvent(event: DomainEvent): void {
    if (event.kind !== 'app_instance_log_appended') return;
    appendRaw(event.instance_id, event.line);
}

export function ensureAppInstanceLogStore(): void {
    if (unsubDomain) return;
    unsubDomain = subscribeDomainEvents(onDomainEvent);
}

export function dropAppInstanceLogs(instanceId: string): void {
    const { byId } = store.getSnapshot();
    if (!(instanceId in byId)) return;
    const next = { ...byId };
    delete next[instanceId];
    store.setState({ byId: next });
}

const hydrating = new Set<string>();

export function hydrateAppInstanceLogs(instanceId: string): void {
    if (!instanceId || hydrating.has(instanceId)) return;
    hydrating.add(instanceId);
    void appFrameworkService
        .tailLog(instanceId, 1000)
        .then((snap) => {
            const historical = buildHistoryEntries(snap.lines ?? []);
            if (historical.length === 0) return;
            store.setState({
                byId: { ...store.getSnapshot().byId, [instanceId]: historical },
            });
        })
        .catch((err) => {
            console.warn('加载应用日志失败:', err);
        })
        .finally(() => {
            hydrating.delete(instanceId);
        });
}

export function clearAppInstanceLogs(instanceId: string): void {
    const { byId } = store.getSnapshot();
    if (!byId[instanceId]?.length) return;
    store.setState({
        byId: { ...byId, [instanceId]: EMPTY },
    });
}

function subscribe(listener: () => void): () => void {
    ensureAppInstanceLogStore();
    return store.subscribe(listener);
}

export const appInstanceLogStore = {
    getSnapshot: store.getSnapshot,
    subscribe,
    _reset(): void {
        if (unsubDomain) {
            try {
                unsubDomain();
            } catch {
                /* noop */
            }
            unsubDomain = null;
        }
        hydrating.clear();
        store._reset();
    },
};

export function useAppInstanceLog(instanceId: string | null) {
    useEffect(() => {
        if (instanceId) hydrateAppInstanceLogs(instanceId);
    }, [instanceId]);
    const snapshot = useSyncExternalStore(subscribe, store.getSnapshot, store.getSnapshot);
    const logs = useMemo(() => {
        if (!instanceId) return EMPTY;
        const raw = snapshot.byId[instanceId];
        if (!raw?.length) return EMPTY;
        return raw.map(canonicalizeLogEntry);
    }, [instanceId, snapshot]);
    const clear = useCallback(() => {
        if (instanceId) clearAppInstanceLogs(instanceId);
    }, [instanceId]);
    return { logs, clear };
}
