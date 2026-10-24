// Docker 管理页的主机选择跨路由保留，部署区返回时继续管理同一台主机。
import { useMemo, useSyncExternalStore } from 'react';

let navigator: (() => void) | null = null;
let selectedHost: string | null = null;
const listeners = new Set<() => void>();

function getSelectedHost(): string | null {
    return selectedHost;
}

function subscribe(listener: () => void): () => void {
    listeners.add(listener);
    return () => {
        listeners.delete(listener);
    };
}

export function setDockerSelectedHost(hostId: string): void {
    if (selectedHost === hostId) return;
    selectedHost = hostId;
    for (const listener of listeners) listener();
}

export function registerDockerNavigator(fn: () => void): () => void {
    navigator = fn;
    return () => {
        if (navigator === fn) navigator = null;
    };
}

export function openDockerManagement(hostId: string): void {
    setDockerSelectedHost(hostId);
    navigator?.();
}

export function useDockerHostSelection(): {
    selectedHost: string | null;
    setSelectedHost: (hostId: string) => void;
} {
    const hostId = useSyncExternalStore(subscribe, getSelectedHost, getSelectedHost);
    return useMemo(
        () => ({ selectedHost: hostId, setSelectedHost: setDockerSelectedHost }),
        [hostId],
    );
}
