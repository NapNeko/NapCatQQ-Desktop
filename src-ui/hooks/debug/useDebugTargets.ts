// 调试台的 Bot 列表，以及「什么时候该重新问一遍」：Bot 状态、WebUI 端口、登录态变了，
// 列表里的运行/在线标记和各通道的可用性都可能跟着变。

import { useEffect } from 'react';
import { useQuery, type QueryClient, useQueryClient } from '@tanstack/react-query';
import { onebotDebugService } from '../../core/services/onebot-debug.service';
import { subscribeDomainEvents } from '../../core/services/domain-event-hub';
import { errorText } from '../../core/domain/errors';
import { pushErrorBar } from '../ui/pushErrorBar';
import type { DomainEvent } from '../../core/ipc/types';
import type { DebugTarget } from '../../core/ipc/generated/debug/DebugTarget';
import { debugCatalogPrefix, debugChannelsKey, debugSpecPrefix, debugTargetsKey } from './keys';

// 目录和动作说明也可能跟着这些事件变：内部通道没就绪时后端回快照，而且不进失败冷却，
// 指望之后再来问；登录 / WebUI 就绪后不问的话，「按内置目录显示」会一直挂着干净的目录盖着在线版
function invalidateCatalogForBot(client: QueryClient, botId: string): void {
    void client.invalidateQueries({ queryKey: debugCatalogPrefix(botId) });
    void client.invalidateQueries({ queryKey: debugSpecPrefix(botId) });
}

function invalidateForEvent(client: QueryClient, event: DomainEvent): void {
    switch (event.kind) {
        case 'bot_state_changed': {
            void client.invalidateQueries({ queryKey: debugTargetsKey });
            void client.invalidateQueries({ queryKey: debugChannelsKey(event.snapshot.bot_id) });
            invalidateCatalogForBot(client, event.snapshot.bot_id);
            break;
        }
        case 'napcat_webui_available':
            void client.invalidateQueries({ queryKey: debugChannelsKey(event.bot_id) });
            invalidateCatalogForBot(client, event.bot_id);
            break;
        // SnowLuma 的内部通道要等登录并认出 uin 才可用；同时列表里的「在线」标记也跟着变
        case 'snowluma_login_state_changed':
        case 'snowluma_uin_detected':
            void client.invalidateQueries({ queryKey: debugTargetsKey });
            void client.invalidateQueries({ queryKey: debugChannelsKey(event.bot_id) });
            invalidateCatalogForBot(client, event.bot_id);
            break;
        // 列表里的「在线」标记来自上游登录状态；NapCat 上线 / 被踢下线都会改它
        case 'napcat_login_online':
        case 'napcat_login_invalidated':
            void client.invalidateQueries({ queryKey: debugTargetsKey });
            if (event.kind === 'napcat_login_online') invalidateCatalogForBot(client, event.bot_id);
            break;
        default:
            break;
    }
}

// 顶栏、左栏都会用这个 hook：事件只订一份，按 QueryClient 引用计数，最后一个卸载时才退订，
// 否则同一个事件会让同一个查询失效好几次、飞行中的请求被反复取消重发。
const bridges = new WeakMap<QueryClient, { refs: number; off: () => void }>();

function acquireBridge(client: QueryClient): () => void {
    let bridge = bridges.get(client);
    if (!bridge) {
        bridge = {
            refs: 0,
            off: subscribeDomainEvents((event) => invalidateForEvent(client, event)),
        };
        bridges.set(client, bridge);
    }
    bridge.refs += 1;
    const held = bridge;
    return () => {
        held.refs -= 1;
        if (held.refs === 0) {
            held.off();
            bridges.delete(client);
        }
    };
}

export function useDebugTargets() {
    const client = useQueryClient();
    useEffect(() => acquireBridge(client), [client]);

    return useQuery<DebugTarget[], Error>({
        queryKey: debugTargetsKey,
        queryFn: async () => {
            try {
                return await onebotDebugService.targets();
            } catch (err) {
                pushErrorBar({
                    key: 'debug-targets',
                    title: '读取 Bot 列表失败',
                    raw: errorText(err),
                });
                throw err instanceof Error ? err : new Error(errorText(err));
            }
        },
        // 事件负责让它失效；这个时长只是兜底，回到页面时不必每次都重拉
        staleTime: 30_000,
    });
}
