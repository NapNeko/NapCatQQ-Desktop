// Tauri 事件流订阅服务。
//
// 唯一持有所有 Tauri event name 字符串的位置。
// 业务侧应经 domain-event-hub / useDomainEvents 订阅，不要直接 subscribe，
// 否则每个调用方都会再订一遍 DOMAIN_EVENT_NAMES 全量通道。

import { isTauri, listen } from '../ipc/transport';
import type { DomainEvent } from '../ipc/types';
import { subscribeMockEvents } from '../ipc/mock/events.mock';

export type DomainEventCallback = (event: DomainEvent) => void;
export type UnsubscribeFn = () => void;

// 后端会广播的 Tauri 事件名清单。新增事件类型时只改这里。
const DOMAIN_EVENT_NAMES = [
    'bot_state_changed',
    'bot_status_changed',
    'bot_log_appended',
    'bot_error',
    'task_progress',
    'napcat_webui_available',
    'bot_process_exited',
    'napcat_login_qrcode',
    'napcat_login_qrcode_removed',
    'napcat_login_online',
    'napcat_login_invalidated',
    'snowluma_daemon_state_changed',
    'snowluma_bot_injected',
    'snowluma_uin_detected',
    'snowluma_login_state_changed',
    'snowluma_pid_set_changed',
    'snowluma_daemon_log',
    'snowluma_docker_endpoints_ready',
    'component_action_progress',
    'docker_deploy_progress',
    'docker_install_progress',
    'deployment_task_changed',
    'deployment_task_removed',
    'desktop_log_appended',
    // Host 连接健康事件（不绑 bot，绑 server_id）
    'host_connection_lost',
    'host_connection_recovered',
] as const;

export const eventStreamService = {
    // 订阅一份合并的 DomainEvent 流。
    // 浏览器预览模式自动 fallback 到 mock 周期事件。
    subscribe: async (callback: DomainEventCallback): Promise<UnsubscribeFn> => {
        if (!isTauri) {
            return subscribeMockEvents(callback);
        }
        const unlisteners: UnsubscribeFn[] = [];
        for (const name of DOMAIN_EVENT_NAMES) {
            try {
                const unlisten = await listen<DomainEvent>(name, (payload) => {
                    callback(payload);
                });
                unlisteners.push(unlisten);
            } catch (err) {
                // eslint-disable-next-line no-console
                console.error(`[event-stream] failed to subscribe ${name}:`, err);
            }
        }
        return () => {
            for (const u of unlisteners) {
                try {
                    u();
                } catch {
                    /* noop */
                }
            }
        };
    },
};

// 浏览器预览模式下，给 services 内部的 mock 状态机用：手工广播一条事件。
// 只 re-export 供 mock 内部调用，不打算给业务代码用。
export { emitMockEvent } from '../ipc/mock/events.mock';
