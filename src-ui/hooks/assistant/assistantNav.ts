// Bot 卡片与诊断助手之间的目标导航。

import { useSyncExternalStore } from 'react';

let navigate: (() => void) | null = null;
let pendingBot: string | null = null;
const listeners = new Set<() => void>();
const emit = () => listeners.forEach((listener) => listener());

export function registerAssistantNavigator(next: () => void): () => void {
    navigate = next;
    return () => {
        if (navigate === next) navigate = null;
    };
}

export function openAssistant(botId?: string): void {
    pendingBot = botId ?? null;
    emit();
    navigate?.();
}

export function consumeAssistantTarget(): string | null {
    const target = pendingBot;
    pendingBot = null;
    emit();
    return target;
}

export function useAssistantTarget(): string | null {
    return useSyncExternalStore(
        (listener) => {
            listeners.add(listener);
            return () => {
                listeners.delete(listener);
            };
        },
        () => pendingBot,
        () => null,
    );
}
