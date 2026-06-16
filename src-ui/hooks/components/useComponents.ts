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
//   - catalogQuery 失败 = 整页爆，由 useComponentPageAlerts 推全局 InfoBar
//   - 单个 detectQuery 失败 = 该行 unknown，由 useComponentPageAlerts 推 InfoBar（当前主机）
//
// frontend-layering：本 hook 唯一允许调 service 的位置。

import { useQuery, useQueries, useQueryClient, type UseQueryResult } from '@tanstack/react-query';
import { useEffect, useMemo } from 'react';
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
import { errorText } from '../../core/domain/errors';
import { mockHosts } from '../../core/ipc/mock/component.mock';
import type { Os, ComponentInfo, ComponentDetectResult } from '../../core/ipc/types';
import type { ServerProfile } from '../../core/ipc/generated/domain/ServerProfile';
import { useIsHostReachable } from '../remote/useIsHostReachable';

type DetectQuery = UseQueryResult<ComponentDetectResult, Error>;

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
            // 本机无 transport 问题，显式标 connected 便于上层统一判断
            state: 'connected',
        };
        const remotes: HostInfo[] = (serversQuery.data ?? []).map((p: ServerProfile) => ({
            host_id: `remote:${p.id}`,
            display_name: p.name || p.host,
            os: 'linux' as Os,
            locality: 'remote',
            state: p.state,
            health: p.health
                ? {
                      consecutiveFailures: p.health.consecutiveFailures,
                      lastFailureReason: p.health.lastFailureReason ?? null,
                      lastFailureAt: p.health.lastFailureAt ?? null,
                  }
                : undefined,
        }));
        return [local, ...remotes];
    }, [serversQuery.data]);

    return { hosts, servers: serversQuery.data ?? [] };
}

export interface UseComponentsResult {
    view: ComponentsView;
    /// 当前已知主机列表（本机 + 已连远端），给 Docker 卡片 / 部署形态复用。
    hosts: HostInfo[];
    isLoading: boolean;
    /** 仅 catalog 加载失败才有值；detect 单点失败下沉到 row.status。 */
    error: Error | null;
    /// 触发整页重新拉一次。
    refetch: () => void;
}

// 取数层：服务器列表 + 自动连接 + catalog + 逐主机 detect。
//
// 抽成独立 hook 让"组件页视图构建"和"启动期后台预热"共享同一套 query。
// react-query 按 query key 去重 + 共享缓存，所以预热 hook 先把数据拉热，
// 进页面的 useComponents 直接命中缓存秒开，不用再等一轮 SSH 探测。
interface ComponentsData {
    hosts: HostInfo[];
    components: ComponentInfo[];
    detectQueries: DetectQuery[];
    catalogError: unknown;
    catalogLoading: boolean;
    refetch: () => void;
}

// 自动连接去重 + 失败冷却（避免 SSH 不可达时每秒重试刷日志）。
const autoConnectInFlight = new Set<string>();
const AUTO_CONNECT_COOLDOWN_MS = 90_000;
const autoConnectCooldownUntil = new Map<string, number>();

function autoConnectBlocked(serverId: string): boolean {
    const until = autoConnectCooldownUntil.get(serverId) ?? 0;
    return Date.now() < until;
}

function useComponentsData(): ComponentsData {
    const { hosts, servers } = useKnownHosts();
    const queryClient = useQueryClient();

    // P0-11：对所有已知主机做传输层可达性判断（本机恒 true，远端看 ServerProfile.state）。
    // 必须在顶层以稳定顺序调用 hook，不能在 map/filter 里条件式调用。
    // 结果用于下面 detectQueries 的 enabled 过滤，以及可能的 UI 区分。
    const hostReachability: Record<string, boolean> = {};
    for (const h of hosts) {
        // eslint-disable-next-line react-hooks/rules-of-hooks
        hostReachability[h.host_id] = useIsHostReachable(h.host_id);
    }

    // 自动连接：对所有 ServerState=disconnected/failed 的远端 host 触发一次
    // test_server_connection（密码用 keyring 缓存）。后端 detect 命令内部也有
    // resolve_host_with_autoconnect 兜底，本 effect 是双重保险。
    //
    // 去重用模块级 Set 而非实例 ref：预热 hook 和组件页可能同时挂载，两个实例
    // 共享去重表才不会对同一台远端各发一次 testConnection（又退化成并发握手）。
    useEffect(() => {
        if (!isTauri) return;
        for (const profile of servers) {
            if (profile.state === 'connected' || profile.state === 'connecting') continue;
            // 已失败的主机不自动重试，避免公钥/密码错误时刷 SSH 日志；用户去远端页手动测。
            if (profile.state === 'failed') continue;
            if (autoConnectBlocked(profile.id)) continue;
            if (autoConnectInFlight.has(profile.id)) continue;
            autoConnectInFlight.add(profile.id);
            serverService
                .testConnection(profile.id)
                .then((report) => {
                    if (report.success) {
                        autoConnectCooldownUntil.delete(profile.id);
                        queryClient.invalidateQueries({ queryKey: ['componentDetect'] });
                        queryClient.invalidateQueries({ queryKey: ['servers'] });
                    } else {
                        autoConnectCooldownUntil.set(
                            profile.id,
                            Date.now() + AUTO_CONNECT_COOLDOWN_MS,
                        );
                    }
                })
                .catch(() => {
                    autoConnectCooldownUntil.set(
                        profile.id,
                        Date.now() + AUTO_CONNECT_COOLDOWN_MS,
                    );
                })
                .finally(() => {
                    autoConnectInFlight.delete(profile.id);
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
    //
    // P0-11：对远端主机，transport 不可达（state === 'failed'）时不发探测请求。
    // enabled 由 hostReachability 控制。本机和可达远端正常探测。
    const detectQueries = useQueries({
        queries: components.flatMap((c) =>
            hosts.map((h) => ({
                queryKey: ['componentDetect', c.id, h.host_id],
                queryFn: () => componentService.detectComponent(c.id, h.host_id),
                enabled: hostReachability[h.host_id] ?? true,
                staleTime: 30 * 1000,
            })),
        ),
    });

    return {
        hosts,
        components,
        detectQueries,
        catalogError: catalogQuery.error,
        catalogLoading: catalogQuery.isLoading,
        refetch: () => {
            void queryClient.invalidateQueries({ queryKey: ['servers'] });
            void queryClient.invalidateQueries({ queryKey: ['docker'] });
            void queryClient.invalidateQueries({ queryKey: ['docker', 'containers'] });
            void catalogQuery.refetch();
            for (const q of detectQueries) void q.refetch();
        },
    };
}

/// 启动期后台预热：在 App 根节点常驻挂载，程序一起来就开始拉服务器列表、
/// 自动连接远端、拉 catalog、逐主机 detect。永不卸载，保持缓存新鲜；用户切到
/// 组件页时 useComponents 直接命中缓存，不再从零等一轮 SSH 探测。
/// dev 可设 VITE_SKIP_COMPONENTS_WARMUP=1 减少改 UI 时 HMR 后的 IPC 风暴。
export function useComponentsWarmup(): void {
    if (import.meta.env.DEV && import.meta.env.VITE_SKIP_COMPONENTS_WARMUP === '1') {
        return;
    }
    useComponentsData();
}

export function useComponents(): UseComponentsResult {
    const { hosts, components, detectQueries, catalogError, catalogLoading, refetch } =
        useComponentsData();

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

                // detect 还在跑（首次加载或 invalidate 后重新拉）：状态改为
                // "正在探测"，不要让 UI 一直显示"尚未探测"误以为出了错。
                // isFetching 覆盖 isLoading + 重新拉取场景；只在 detect 为 null
                // 时才覆盖，已经拿到 detect 数据后即使 isFetching 也保留旧值。
                if (status.state === 'unknown' && detect === null && detectQuery?.isFetching) {
                    status = {
                        state: 'unknown',
                        reason: '正在探测',
                    };
                }

                // detect 真失败时把错误信息塞进 reason，让用户能看到具体原因
                // （如 "自动连接被拒绝…" / "ssh connect timeout"）。后端返回的是
                // Err(String)，invoke reject 出来是裸字符串而非 Error，必须走
                // errorText 正规化，否则 (err as Error).message 取到 undefined
                // 会把真因吞成空白。
                if (status.state === 'unknown' && detectQuery?.error) {
                    status = {
                        state: 'unknown',
                        reason: errorText(detectQuery.error, '探测失败'),
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

    // catalog 失败 = 整页爆。detect 单点失败下沉到 row，由页面 hook 转 InfoBar。
    // catalogError 同样可能是裸字符串，正规化成带 message 的 Error，
    // 让顶部 banner（读 error.message）能显示真因。
    const error = catalogError ? new Error(errorText(catalogError, '加载组件清单失败')) : null;

    const isLoading =
        catalogLoading ||
        detectQueries.some((q) => q.isLoading);

    return {
        view,
        hosts,
        isLoading,
        error,
        refetch,
    };
}
