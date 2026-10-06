// 接口目录和单个动作的说明。
//
// 目录来源随 Bot 是否在线变化（在线时是上游现取的，否则是随桌面端打包的快照），
// 所以按 Bot 缓存、永不自动过期，只在该 Bot 的运行状态翻转时失效重取。
// 没选 Bot 时（bot_id 为 null）按后端类型看内置快照。

import { useEffect } from 'react';
import { useQuery, useQueryClient, type QueryClient } from '@tanstack/react-query';
import { onebotDebugService } from '../../core/services/onebot-debug.service';
import { errorText } from '../../core/domain/errors';
import { pushErrorBar } from '../ui/pushErrorBar';
import type { BackendType } from '../../core/ipc/generated/domain/BackendType';
import type { DebugActionSpec } from '../../core/ipc/generated/debug/DebugActionSpec';
import type { DebugCatalog } from '../../core/ipc/generated/debug/DebugCatalog';
import {
    debugCatalogKey,
    debugCatalogPrefix,
    debugIdleKey,
    debugSpecKey,
    debugSpecPrefix,
} from './keys';

/** `DebugTarget` 可以直接传；没选 Bot 时给 `{ bot_id: null, backend }` */
export interface DebugCatalogTarget {
    bot_id: string | null;
    backend: BackendType;
    running?: boolean;
}

// 每个 Bot 上次见到的运行状态。放在模块里而不是 ref 里：页面卸载期间 Bot 停了，
// 再回来时缓存里还是在线时取的目录，要靠这份记录认出「翻转过」。
// 多个组件同时用 useDebugCatalog / useDebugActionSpec 时也只有第一个观察到翻转的会去失效。
const lastRunning = new Map<string, boolean>();

function noteRunning(
    client: QueryClient,
    botId: string | null,
    running: boolean | undefined,
): void {
    if (botId === null || running === undefined) return;
    const before = lastRunning.get(botId);
    lastRunning.set(botId, running);
    if (before === undefined || before === running) return;
    void client.invalidateQueries({ queryKey: debugCatalogPrefix(botId) });
    void client.invalidateQueries({ queryKey: debugSpecPrefix(botId) });
}

function useInvalidateOnRunningFlip(target: DebugCatalogTarget | null): void {
    const client = useQueryClient();
    const botId = target?.bot_id ?? null;
    const running = target?.running;
    useEffect(() => {
        noteRunning(client, botId, running);
    }, [client, botId, running]);
}

// 内部通道还没就绪（SL 还没登录、NC 的 WebUI 还没起来）时后端回快照，而且不进失败冷却，
// 指望之后再问。Bot 在跑、目录却还是快照时，过一阵就再问一次：登录 / WebUI 就绪后目录
// 自己换到在线版，不用等重启或事件翻转。这份重拉也顺带越过后端那次拉取的失败冷却
const SNAPSHOT_REFRESH_MS = 60_000;

export function useDebugCatalog(target: DebugCatalogTarget | null) {
    useInvalidateOnRunningFlip(target);
    const botId = target?.bot_id ?? null;
    const backend = target?.backend;
    const running = target?.running === true;
    return useQuery<DebugCatalog, Error>({
        queryKey: backend ? debugCatalogKey(botId, backend) : debugIdleKey,
        queryFn: async () => {
            if (!backend) throw new Error('未选择目标');
            try {
                return await onebotDebugService.catalog(botId, backend);
            } catch (err) {
                pushErrorBar({
                    key: `debug-catalog:${botId ?? backend}`,
                    title: '读取接口目录失败',
                    raw: errorText(err),
                });
                throw err instanceof Error ? err : new Error(errorText(err));
            }
        },
        enabled: !!backend,
        staleTime: Infinity,
        refetchInterval: (query) =>
            running && query.state.data?.source === 'snapshot' ? SNAPSHOT_REFRESH_MS : false,
    });
}

/** 动作不存在时数据是 null（比如换了后端后旧标签里的动作） */
export function useDebugActionSpec(target: DebugCatalogTarget | null, name: string | null) {
    useInvalidateOnRunningFlip(target);
    const botId = target?.bot_id ?? null;
    const backend = target?.backend;
    const running = target?.running === true;
    const on = !!backend && !!name;
    return useQuery<DebugActionSpec | null, Error>({
        queryKey: on ? debugSpecKey(botId, backend, name) : debugIdleKey,
        queryFn: async () => {
            if (!backend || !name) return null;
            try {
                return await onebotDebugService.describe(botId, backend, name);
            } catch (err) {
                pushErrorBar({
                    key: `debug-spec:${botId ?? backend}:${name}`,
                    title: `读取接口 ${name} 的说明失败`,
                    raw: errorText(err),
                });
                throw err instanceof Error ? err : new Error(errorText(err));
            }
        },
        enabled: on,
        staleTime: Infinity,
        // 和目录同一个理由：还挂着快照说明时定时再问一次，换到在线版后停下来
        refetchInterval: (query) =>
            running && query.state.data?.source === 'snapshot' ? SNAPSHOT_REFRESH_MS : false,
    });
}

/** 测试用 */
export function _resetDebugCatalogForTests(): void {
    lastRunning.clear();
}
