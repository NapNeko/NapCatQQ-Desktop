// 跨页新建 Docker Bot 时保留主机与框架，只预填配置，不执行部署。
import { useSyncExternalStore } from 'react';
import { createDefaultBotConfig } from '../../core/domain/bot/config-defaults';
import {
    isRuntimeTargetConcreteRemote,
    normalizeRuntimeTargetFromDisk,
} from '../../core/domain/bot/runtime-target';
import type { DockerFlavor } from '../../core/ipc/types';
import type { BotConfig } from '../../core/ipc/generated/domain/BotConfig';
import { featureTogglesStore } from '../preferences/featureTogglesStore';
import { pushInfoBar } from '../ui/globalInfoBarStore';
import { localOs } from '../../core/domain/runtime/env';

export interface DockerBotCreateTarget {
    hostId: string;
    flavor: DockerFlavor;
}

export interface DockerBotCreateRequest extends DockerBotCreateTarget {
    requestId: number;
}

let navigator: (() => void) | null = null;
let pendingRequest: DockerBotCreateRequest | null = null;
let nextRequestId = 1;
const listeners = new Set<() => void>();

function emit(): void {
    for (const listener of listeners) listener();
}

export function registerBotCreateNavigator(fn: () => void): () => void {
    navigator = fn;
    return () => {
        if (navigator === fn) navigator = null;
    };
}

export function dockerBotRuntimeTarget(hostId: string): string | null {
    if (hostId === 'local') return localOs() === 'linux' ? 'local' : null;
    if (!hostId.startsWith('remote:') || !hostId.slice('remote:'.length).trim()) return null;
    const target = normalizeRuntimeTargetFromDisk(hostId);
    return target.trim() && isRuntimeTargetConcreteRemote(target) ? target : null;
}

export function openCreateDockerBot(target: DockerBotCreateTarget): void {
    const framework = target.flavor === 'napcat' ? 'NapCat' : 'SnowLuma';
    const reason = !dockerBotRuntimeTarget(target.hostId)
        ? '请选择本机 Linux 或一台 Linux 远程主机后再新建 Docker Bot。'
        : !featureTogglesStore.getSnapshot()[target.flavor]
          ? `请先在设置中启用 ${framework}。`
          : null;
    if (reason) {
        if (pendingRequest) {
            pendingRequest = null;
            emit();
        }
        pushInfoBar({ tone: 'warning', title: '无法新建 Docker Bot', content: reason });
        return;
    }
    pendingRequest = { ...target, requestId: nextRequestId++ };
    emit();
    navigator?.();
}

export function getPendingBotCreateRequest(): DockerBotCreateRequest | null {
    return pendingRequest;
}

export function consumePendingBotCreateRequest(requestId: number): boolean {
    if (pendingRequest?.requestId !== requestId) return false;
    pendingRequest = null;
    emit();
    return true;
}

function subscribe(listener: () => void): () => void {
    listeners.add(listener);
    return () => listeners.delete(listener);
}

export function usePendingBotCreateRequest(): DockerBotCreateRequest | null {
    return useSyncExternalStore(subscribe, getPendingBotCreateRequest, getPendingBotCreateRequest);
}

export function createDockerBotConfig(target: DockerBotCreateTarget): BotConfig {
    const runtimeTarget = dockerBotRuntimeTarget(target.hostId);
    if (!runtimeTarget) throw new Error('Docker Bot 必须使用 Linux 主机');
    const config = createDefaultBotConfig();
    config.bot.backend_type = target.flavor;
    config.bot.runtime_target = runtimeTarget;
    config.bot.deploymentType = 'docker';
    return config;
}
