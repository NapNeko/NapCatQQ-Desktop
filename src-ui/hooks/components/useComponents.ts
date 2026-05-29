// Components 页面主 hook。
//
// 职责：
//   1. 拉一次 list_components（react-query 缓存）
//   2. 对每个 (component, host) 组合 detect_component，merge 成 ComponentRow[]
//   3. 暴露 refetch / lastError 给 UI
//
// 主机来源策略：
//   - Tauri 模式：当前后端没有 list_remote_hosts command，远端注册表 v1
//     未实装；先只暴露本机 host。等后端把远端注册表接进来后扩成
//     [local, ...listRemoteHosts()] 即可。
//   - 浏览器预览：用 mockHosts 全集（local + 2 remote），便于演示多主机 UI。
//
// 错误分层：
//   - catalogQuery 失败 = 整页爆，往外抛 error，UI 顶部 banner 显示
//   - 单个 detectQuery 失败 = 那一行 host 的事，下沉到 row.status.unknown.reason
//     让用户在那一行看到真错误（"remote host registry not implemented"），而不
//     是误以为"整个清单都没拉到"
//
// frontend-layering：本 hook 唯一允许调 service 的位置。

import { useQuery, useQueries } from '@tanstack/react-query';
import { useEffect, useMemo, useRef } from 'react';
import { isTauri } from '../../core/ipc/transport';
import { componentService } from '../../core/services/component.service';
import { serverService } from '../../core/services/server.service';
import {
    deriveStatus,
    splitByCategory,
    type ComponentRow,
    type ComponentsView,
    type HostInfo,
} from '../../core/domain/components/types';
import { mockHosts } from '../../core/ipc/mock/component.mock';
import type { Os } from '../../core/ipc/types';
import type { ServerProfile } from '../../core/ipc/generated/domain/ServerProfile';
import { useQueryClient } from '@tanstack/react-query';

// 探测本机 OS。当前 Tauri 工程仅 Windows 编译 LocalHost（src-tauri/src/commands/
// components.rs::local_host 上有 #[cfg(windows)]），所以 Windows 兜底是安全的；
// 但保留 ua sniffing 让未来在 macOS / Linux 桌面跑起来时也能正确派生。
function detectLocalOs(): Os {
    if (typeof navigator === 'undefined') return 'windows';
    const ua = navigator.userAgent.toLowerCase();
    if (ua.includes('mac os') || ua.includes('macintosh')) return 'mac_os';
    if (ua.includes('linux')) return 'linux';
    return 'windows';
}

/// Tauri 模式下从 ServerManager 拉服务器档案，组合本机 + 远端 host 列表。
/// 浏览器预览用 mockHosts。
function useKnownHosts(): { hosts: HostInfo[]; servers: ServerProfile[] } {
    const serversQuery = useQuery({
        queryKey: ['servers'],
        queryFn: () => serverService.list(),
        enabled: isTauri,
    });

    const hosts = useMemo<HostInfo[]>(() => {
        if (!isTauri) {
            return mockHosts;
        }
        const local: HostInfo = {
            host_id: 'local',
            display_name: '本机',
            os: detectLocalOs(),
            locality: 'local',
        };
        const remotes: HostInfo[] = (serversQuery.data ?? []).map((p) => ({
            host_id: `remote:${p.id}`,
            display_name: p.name || p.host,
            // 远端默认 Linux —— RemoteLinuxHost 是当前唯一实装。后续接入
            // RemoteWindowsHost 时按 ServerProfile 增加 os 字段。
            os: 'linux' as Os,
            locality: 'remote',
        }));
        return [local, ...remotes];
    }, [serversQuery.data]);

    return { hosts, servers: serversQuery.data ?? [] };
}

export interface UseComponentsResult {
    view: ComponentsView;
    isLoading: boolean;
    /** 仅 catalog 加载失败才有值；detect 单点失败下沉到 row.status。 */
    error: Error | null;
    /// 触发整页重新拉一次。
    refetch: () => void;
}

export function useComponents(): UseComponentsResult {
    const { hosts, servers } = useKnownHosts();
    const queryClient = useQueryClient();

    // 自动连接：进入组件页时，对所有 ServerState=disconnected/failed 的远端 host
    // 触发一次 test_server_connection（密码用 keyring 缓存），让组件 detect 不
    // 报"未连接"。已经是 connected/connecting 的跳过，避免重复打扰。
    // useRef 防止 effect 在 servers 数组身份变化时重复触发同一台主机。
    const autoConnectedRef = useRef<Set<string>>(new Set());
    useEffect(() => {
        if (!isTauri) return;
        for (const profile of servers) {
            if (profile.state === 'connected' || profile.state === 'connecting') continue;
            if (autoConnectedRef.current.has(profile.id)) continue;
            autoConnectedRef.current.add(profile.id);
            serverService
                .testConnection(profile.id)
                .then((report) => {
                    if (report.success) {
                        // 连接建立后让 detect 矩阵重新拉一遍。
                        queryClient.invalidateQueries({ queryKey: ['componentDetect'] });
                        queryClient.invalidateQueries({ queryKey: ['servers'] });
                    }
                })
                .catch(() => {
                    // 凭据缺失 / 网络错误等：让用户去远端页手动测试，组件页不弹红。
                });
        }
    }, [servers, queryClient]);

    const catalogQuery = useQuery({
        queryKey: ['componentCatalog'],
        queryFn: componentService.listComponents,
        staleTime: 5 * 60 * 1000,
    });

    const components = catalogQuery.data ?? [];

    // 对每个 (component, host) 发一个 detect。
    // useQueries 让 react-query 自己管理缓存 + 并发；keys 稳定。
    const detectQueries = useQueries({
        queries: components.flatMap((c) =>
            hosts.map((h) => ({
                queryKey: ['componentDetect', c.id, h.host_id],
                queryFn: () => componentService.detectComponent(c.id, h.host_id),
                staleTime: 30 * 1000,
            })),
        ),
    });

    const view = useMemo<ComponentsView>(() => {
        if (components.length === 0) {
            return { framework: [], runtimeDep: [], selfApp: [] };
        }

        const hostCount = hosts.length;
        const rows: ComponentRow[] = components.map((info, ci) => ({
            info,
            rows: hosts.map((host, hi) => {
                const detectQuery = detectQueries[ci * hostCount + hi];
                const detect = detectQuery?.data ?? null;
                let status = deriveStatus(host, info, detect);

                // detect 失败时把真错误信息塞进 row 的 unknown.reason，让用户
                // 在那一行能看到 "remote host registry not implemented" 之类的具体
                // 原因，而不是错误地以为还在 loading。
                if (status.state === 'unknown' && detectQuery?.error) {
                    const err = detectQuery.error as Error;
                    status = {
                        state: 'unknown',
                        reason: err.message || '探测失败',
                    };
                }

                return {
                    component_id: info.id,
                    host,
                    status,
                };
            }),
        }));

        return splitByCategory(rows);
    }, [components, hosts, detectQueries]);

    // catalog 失败 = 整页爆。detect 单点失败下沉到 row 不往外抛。
    const error = (catalogQuery.error as Error | null) ?? null;

    const isLoading =
        catalogQuery.isLoading ||
        detectQueries.some((q) => q.isLoading);

    return {
        view,
        isLoading,
        error,
        refetch: () => {
            void catalogQuery.refetch();
            for (const q of detectQueries) void q.refetch();
        },
    };
}
