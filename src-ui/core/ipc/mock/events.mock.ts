// 浏览器预览模式下的事件流模拟器。
//
// 真 Tauri 事件订阅在 `core/services/event-stream.service.ts`。
// 这里只在非 Tauri 环境下生成周期性 mock 事件，以及作为
// botCommands mock 调用时手工 emit 状态变更事件的总线。

import type { DomainEvent } from '../types';

export type EventCallback = (event: DomainEvent) => void;
export type UnsubscribeFn = () => void;

const activeMockCallbacks = new Set<EventCallback>();

/// 任何 mock 状态机迁移后调用，把事件广播给所有订阅者。
export function emitMockEvent(event: DomainEvent): void {
    for (const cb of activeMockCallbacks) {
        try {
            cb(event);
        } catch (err) {
            // eslint-disable-next-line no-console
            console.error('Error invoking mock event callback:', err);
        }
    }
}

/// 浏览器预览模式订阅。返回 unsubscribe。
/// 同时启动几个周期性 mock 事件源（task_progress / bot_log_appended）。
export function subscribeMockEvents(callback: EventCallback): UnsubscribeFn {
    activeMockCallbacks.add(callback);
    let isUnsubscribed = false;
    const timers: Array<ReturnType<typeof setTimeout | typeof setInterval>> = [];

    // 1) 启动后 1s 触发一次迁移完成事件。
    timers.push(
        setTimeout(() => {
            if (isUnsubscribed) return;
            callback({
                kind: 'task_progress',
                task_id: 'boot-migration',
                progress: 100,
                message: '数据层 V2 -> V3 迁移检测完成。',
            });
        }, 1000),
    );

    // 2) 周期性日志行。
    timers.push(
        setInterval(() => {
            if (isUnsubscribed) return;
            const botIds = ['10001', '10002'];
            const randomBot = botIds[Math.floor(Math.random() * botIds.length)];
            const logLines = [
                '[NapCat] [INFO] WebSocket service listening on port 3001',
                '[NapCat] [INFO] Connected to QQ server successfully',
                '[NapCat] [DEBUG] Syncing contacts... 45%',
                '[NapCat] [INFO] OneBot11 api call: get_login_info',
                '[NapCat] [WARN] Connection to gateway lost, retrying in 5s...',
                '[NapCat] [INFO] Reconnected to gateway',
            ];
            const line = logLines[Math.floor(Math.random() * logLines.length)];
            callback({
                kind: 'bot_log_appended',
                bot_id: randomBot,
                line,
                channel: 'stdout',
            });
        }, 3500),
    );

    // 3) 周期性进度更新。
    timers.push(
        setInterval(() => {
            if (isUnsubscribed) return;
            const progress = Math.floor(Math.random() * 100);
            callback({
                kind: 'task_progress',
                task_id: 'remote-tunnel',
                progress,
                message: `正在刷新远端隧道连接缓存 (${progress}%)`,
            });
        }, 8000),
    );

    return () => {
        isUnsubscribed = true;
        activeMockCallbacks.delete(callback);
        for (const id of timers) {
            clearTimeout(id as ReturnType<typeof setTimeout>);
            clearInterval(id as ReturnType<typeof setInterval>);
        }
    };
}
