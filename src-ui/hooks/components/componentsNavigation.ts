// 跨页打开指定主机的 Docker 部署区；请求消费后不锁定主机选择。
import { useSyncExternalStore } from 'react';

export interface DockerDeploymentRequest {
    requestId: number;
    hostId: string;
}

let navigator: (() => void) | null = null;
let pending: DockerDeploymentRequest | null = null;
let nextRequestId = 1;
const listeners = new Set<() => void>();

function emit(): void {
    for (const listener of listeners) listener();
}

export function registerComponentsNavigator(fn: () => void): () => void {
    navigator = fn;
    return () => {
        if (navigator === fn) navigator = null;
    };
}

export function openDockerDeployment(hostId: string): void {
    pending = { requestId: nextRequestId++, hostId };
    emit();
    navigator?.();
}

export function getPendingDockerDeployment(): DockerDeploymentRequest | null {
    return pending;
}

export function consumeDockerDeployment(requestId: number): void {
    if (pending?.requestId !== requestId) return;
    pending = null;
    emit();
}

export function subscribeDockerDeployment(listener: () => void): () => void {
    listeners.add(listener);
    return () => {
        listeners.delete(listener);
    };
}

export function usePendingDockerDeployment(): DockerDeploymentRequest | null {
    return useSyncExternalStore(
        subscribeDockerDeployment,
        getPendingDockerDeployment,
        getPendingDockerDeployment,
    );
}
