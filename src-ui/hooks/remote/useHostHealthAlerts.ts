// 远端主机健康告警（Transport 层失败的 InfoBar 推送与抑制）。
//
// 职责：
// - 监听 servers 状态变化（或 host_connection_* 事件），做边沿检测。
// - 进入 failed 时，只有 health.consecutiveFailures >= CONSECUTIVE_FAILURES_INFOBAR_THRESHOLD（当前 2）
//   才推 danger InfoBar（key: `host-unreachable:${serverId}`）。
//   低于阈值（短暂抖动、cf=1 即恢复）只改状态/视觉，不推 InfoBar。
// - 用户手动关闭该条（onUserDismiss）时，记录抑制，后续相同失败不再重复推。
// - 恢复（state 变回非 failed，或收到 HostConnectionRecovered）时：清除抑制 + 主动 dismiss 同 key 条。
// - 复用 globalInfoBarStore 的 key 顶替 + onUserDismiss 抑制，与 useComponentPageAlerts 等一致。
// - 常驻挂载（App 根节点），类似 useHostConnectionEvents。
//
// 与 P1 主动探活 walker 协同：后台低频探测会持续递增 consecutiveFailures，真实持续失败才会达到阈值并推送。

import { useEffect, useRef } from 'react';
import { useQuery } from '@tanstack/react-query';
import { serverService } from '../../core/services/server.service';
import { isTauri } from '../../core/ipc/transport';
import { pushInfoBar, dismissInfoBar } from '../ui/globalInfoBarStore';
import { subscribeDomainEvents } from '../../core/services/domain-event-hub';
import type { DomainEvent } from '../../core/ipc/types';

type ServerLike = {
    id: string;
    name?: string | null;
    host?: string | null;
    state: string; // ServerState
    health?: { consecutiveFailures?: number } | null;
};

// 连续失败达到此阈值才推 danger InfoBar，低于阈值只改状态/视觉（抖动抑制）。
// 配合后台 walker（P1 主动探活），连续探测失败才会递增计数，短暂抖动（cf=1 即恢复）不推提示。
const CONSECUTIVE_FAILURES_INFOBAR_THRESHOLD = 2;

// module-level 抑制集合：key 为 serverId，存在即表示该主机当前被抑制（用户已 dismiss 过本次失败）。
const suppressed = new Set<string>();

export function clearHostUnreachableSuppression(serverId: string): void {
    suppressed.delete(serverId);
}

export function isHostUnreachableSuppressed(serverId: string): boolean {
    return suppressed.has(serverId);
}

/**
 * 常驻 hook：监听远端主机状态，边沿检测 failed 变化，推/抑制定制 key 的 InfoBar。
 * 建议在 AppNext 根节点调用一次。
 */
export function useHostHealthAlerts(): void {
    const serversQuery = useQuery({
        queryKey: ['servers'],
        queryFn: () => serverService.list(),
        enabled: isTauri,
        staleTime: 5_000,
    });

    // ref 记录上一次各 server 的 state，用于边沿检测
    const prevStatesRef = useRef<Record<string, string>>({});

    useEffect(() => {
        const servers: ServerLike[] = (serversQuery.data ?? []) as ServerLike[];
        const prev = prevStatesRef.current;

        for (const s of servers) {
            const prevState = prev[s.id] ?? 'disconnected'; // 初始视为非 failed
            const currState = s.state;

            // 记录当前
            prev[s.id] = currState;

            const label = s.name?.trim() || s.host?.trim() || s.id;
            const key = `host-unreachable:${s.id}`;

            if (currState === 'failed' && prevState !== 'failed') {
                // 进入 failed：检查连续失败计数，只有达到阈值才推 InfoBar（短暂抖动 cf=1 不推）。
                const cf = s.health?.consecutiveFailures ?? 0;
                if (cf >= CONSECUTIVE_FAILURES_INFOBAR_THRESHOLD && !suppressed.has(s.id)) {
                    pushInfoBar({
                        key,
                        tone: 'danger',
                        title: '远端主机连接中断',
                        content: `主机 ${label} 无法访问，请检查网络、SSH 配置或凭据。`,
                        closable: true,
                        onUserDismiss: () => {
                            // 用户手动关闭后抑制后续重复推送，直到恢复
                            suppressed.add(s.id);
                        },
                    });
                }
            }

            if (currState !== 'failed' && prevState === 'failed') {
                // 从 failed 恢复 → 清除抑制 + 清理屏幕上可能还存在的同 key 条
                suppressed.delete(s.id);
                // 主动 dismiss（幂等，如果不存在就什么都不做）
                dismissInfoBar(`key:${key}`);
            }
        }
    }, [serversQuery.data]);

    // 额外：直接订阅 host_connection_recovered 事件，加速恢复侧的抑制清除（即使 servers query 还没回）。
    // 这样在事件到达的瞬间就能清抑制并尝试 dismiss，避免等下一次 servers 轮询。
    useEffect(() => {
        const unsub = subscribeDomainEvents((event: DomainEvent) => {
            if (event.kind === 'host_connection_recovered') {
                const serverId: string | undefined = (event as any).server_id;
                if (!serverId) return;

                suppressed.delete(serverId);
                dismissInfoBar(`key:host-unreachable:${serverId}`);
            }

            // 失败事件到达时，如果当前 servers 里该 server 已是 failed 且未抑制，也可在此推（兜底）。
            // 但主要推逻辑已在上面 servers effect 里做，这里只做恢复加速。
        });

        return unsub;
    }, []);
}
