// 判断给定 hostId（'local' 或 'remote:xxx'）当前传输层是否可达。
//
// 用途：
// - 作为 useHostComponentInstalled 等 detect queries 的 enabled 条件，transport 失败时不发探测请求。
// - UI 层（BotCard、IdentityTab、HostComponentsView、ComponentsPage）优先用这个来区分“主机不可达”与“组件缺失”。
//
// 数据源：复用 ['servers'] 查询缓存（由 useServerManager / useComponents 预热）。
// 本机（'local'）恒返回 true。
// 远端：ServerProfile.state !== 'failed' 才算可达（Disconnected / Connecting / Connected 都允许探测，
//       只有显式 Failed 才阻断，避免在抖动恢复窗口里完全不探）。

import { useQuery } from '@tanstack/react-query';
import { serverService } from '../../core/services/server.service';
import { isTauri } from '../../core/ipc/transport';
import type { ServerProfile } from '../../core/ipc/generated/domain/ServerProfile';

/**
 * 判断 host 是否在传输层可达。
 * - hostId === 'local' 或 null/undefined → true（本机无 transport 问题）
 * - hostId === 'remote:<id>' → 查找对应 ServerProfile，state !== 'failed'
 */
export function useIsHostReachable(hostId: string | null | undefined): boolean {
    const serversQuery = useQuery({
        queryKey: ['servers'],
        queryFn: () => serverService.list(),
        enabled: isTauri,
        // 依赖外部事件驱动失效（host_connection_* 到达时 useHostConnectionEvents 会 invalidate）。
        // 这里给一个较长的 staleTime，避免在无事件期间反复打后端；真正新鲜度由事件 + 手动 refetch 保障。
        staleTime: 30_000,
    });

    if (!hostId) return true;
    if (hostId === 'local') return true;

    // remote:xxx
    const serverId = hostId.startsWith('remote:') ? hostId.slice('remote:'.length) : hostId;
    const profile = (serversQuery.data ?? []).find((p: ServerProfile) => p.id === serverId);
    if (!profile) {
        // 档案不存在时，保守认为不可达（上层通常会先提示“请先添加远程主机”）。
        return false;
    }
    return profile.state !== 'failed';
}

/**
 * 同步快照版（仅用于 render 路径中不需要触发新请求的场景）。
 * 依赖调用方所在组件树里已经有人把 ['servers'] 打热，否则返回的可能是空数组导致误判。
 *
 * 谨慎使用；推荐优先用 hook 版（它会确保至少有一个活跃 observer）。
 */
export function isHostReachableFromCache(
    hostId: string | null | undefined,
    servers: Array<{ id: string; state: string }> | undefined,
): boolean {
    if (!hostId) return true;
    if (hostId === 'local') return true;
    const serverId = hostId.startsWith('remote:') ? hostId.slice('remote:'.length) : hostId;
    const profile = (servers ?? []).find((p) => p.id === serverId);
    if (!profile) return false;
    return profile.state !== 'failed';
}
