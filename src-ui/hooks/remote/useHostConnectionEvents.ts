// 远端主机连接健康事件监听 + 自动失效 react-query 缓存。
//
// 监听后端通过 ServerManager 发布的 HostConnectionLost / HostConnectionRecovered 事件，
// 立即失效对应主机的服务器列表和组件探测缓存，让 UI 尽快看到“主机不可达”或恢复后的新鲜状态。
//
// 事件不绑 bot_id，只带 server_id（ServerProfile.id），对应前端 hostId = `remote:${server_id}`。
// 监听方通常是常驻 hook（App 根或预热路径），也可在需要精确刷新的页面局部挂载。
//
// 职责边界：
// - 只负责 invalidateQueries + 在恢复时显式 refetchQueries（触发立即拉取），不负责推 InfoBar。
// - InfoBar 由 useHostHealthAlerts 按三层失败区分 + 抑制策略处理。
// - 不持有长期状态，只做“事件 → 缓存刷新”的边车。

import { useEffect } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { subscribeDomainEvents } from '../../core/services/domain-event-hub';
import type { DomainEvent } from '../../core/ipc/types';

/**
 * 常驻监听远端主机连接健康事件。
 * 建议在 App 根节点或组件预热路径调用一次（幂等，内部用全局订阅分发）。
 *
 * 事件到达时：
 * - invalidate ['servers']（让 useServerManager / ServerCard 看到新的 state/health）
 * - invalidate 该主机的所有 ['componentDetect', *, `remote:${server_id}`]
 * - 特别地：当收到 HostConnectionRecovered 时，**显式 refetchQueries** 强制立即拉取，
 *   解决“远程页测试连接成功后，Bot 列表徽标和配置页启动/保存门禁不自动解除”的问题。
 *
 * 本机 host（'local'）不产生这类事件，忽略。
 */
export function useHostConnectionEvents(): void {
    const queryClient = useQueryClient();

    useEffect(() => {
        const unsubscribe = subscribeDomainEvents((event: DomainEvent) => {
            if (event.kind === 'host_connection_lost' || event.kind === 'host_connection_recovered') {
                // 事件 payload 里 server_id 是 ServerProfile.id，对应前端 hostId = `remote:${server_id}`
                const serverId: string | undefined = (event as any).server_id;
                if (!serverId) return;

                const remoteHostId = `remote:${serverId}`;

                // 1. 让服务器列表（含 state / health）立即刷新
                queryClient.invalidateQueries({ queryKey: ['servers'] });

                // 2. 让该主机的组件探测缓存失效（精确到 hostId 前缀匹配）。
                //    react-query v5 的 invalidate 支持 predicate，这里用部分 key 匹配：
                //    所有 queryKey 以 ['componentDetect', <componentId>, remoteHostId] 开头的都会被命中。
                //    这样比全局 ['componentDetect'] 更精确，不会无谓打扰本机或其它远端。
                queryClient.invalidateQueries({
                    predicate: (query) => {
                        const key = query.queryKey;
                        // 期望结构：['componentDetect', componentId, hostId]
                        return (
                            Array.isArray(key) &&
                            key.length >= 3 &&
                            key[0] === 'componentDetect' &&
                            key[2] === remoteHostId
                        );
                    },
                });

                // 恢复链路增强（核心修复用户报告的问题）：
                // 只 invalidate 有时不够（尤其当相关 observer 有 staleTime、或 Bot 列表/配置页
                // 因为路由切换暂时没有 active observer 时）。
                // 在收到 HostConnectionRecovered 时，显式 refetchQueries 强制立即后台拉取新数据，
                // 确保：
                // - BotCard 里的 useIsHostReachable 尽快返回 true → 解除禁用、移除“主机不可达”chip、恢复正常视觉
                // - useBotRuntimeStartGate 里的 servers 更新 → gateArgs 重算 → runtimeStartBlockReason/saveBlock
                //   不再返回“远端主机 ... 连接中断”，允许保存和启动
                // - 组件页从“主机不可达”全灰占位尽快恢复成正常探测状态
                if (event.kind === 'host_connection_recovered') {
                    void queryClient.refetchQueries({ queryKey: ['servers'] });

                    // 同时加速该主机的 component detect 恢复
                    void queryClient.refetchQueries({
                        predicate: (query) => {
                            const key = query.queryKey;
                            return (
                                Array.isArray(key) &&
                                key.length >= 3 &&
                                key[0] === 'componentDetect' &&
                                key[2] === remoteHostId
                            );
                        },
                    });
                }

                // 可选：如果未来引入了 per-host 健康查询 key（计划里提到 ['hostHealth', id]），
                // 在这里可以追加 invalidate / refetch。
            }
        });

        return unsubscribe;
    }, [queryClient]);
}
