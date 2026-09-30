// 顶栏 Bot 选择器的分组和搜索：按跑在哪台机器分组（本机 / 各远端 / 各远端的 Docker），组内保持后端给的顺序。

import type { DebugHost } from '../../ipc/generated/debug/DebugHost';
import type { DebugTarget } from '../../ipc/generated/debug/DebugTarget';
import type { BackendType } from '../../ipc/generated/domain/BackendType';

export interface TargetGroup {
    /** 分组键：`local` / `remote:<id>` / `docker:<id>` */
    key: string;
    label: string;
    targets: DebugTarget[];
}

function hostKey(host: DebugHost): string {
    switch (host.kind) {
        case 'local':
            return 'local';
        case 'remote':
            return `remote:${host.server_id}`;
        case 'docker':
            return `docker:${host.server_id}`;
        default: {
            const _exhaustive: never = host;
            return _exhaustive;
        }
    }
}

/**
 * `serverName` 把远端 id 换成用户起的名字；换不到就直接显示 id。
 * 组的顺序：本机在最前，其余按第一次出现的顺序。
 */
export function groupTargets(
    targets: readonly DebugTarget[],
    serverName: (serverId: string) => string | undefined = () => undefined,
): TargetGroup[] {
    const groups = new Map<string, TargetGroup>();
    for (const t of targets) {
        const key = hostKey(t.host);
        let group = groups.get(key);
        if (!group) {
            const label =
                t.host.kind === 'local'
                    ? '本机'
                    : `${t.host.kind === 'remote' ? '远端' : 'Docker'} · ${serverName(t.host.server_id) ?? t.host.server_id}`;
            group = { key, label, targets: [] };
            groups.set(key, group);
        }
        group.targets.push(t);
    }
    const list = [...groups.values()];
    const local = list.findIndex((g) => g.key === 'local');
    if (local > 0) list.unshift(...list.splice(local, 1));
    return list;
}

/** 配置里没起名字的 Bot 用 QQ 号（再没有就用 bot_id）顶上，和 Bot 卡片一致 */
export function targetDisplayName(t: DebugTarget): string {
    const name = t.name.trim();
    if (name) return name;
    return t.qq_id > 0 ? String(t.qq_id) : t.bot_id;
}

/** 名字、QQ 号、bot_id 里包含关键字就算命中（不分大小写） */
export function filterTargets(targets: readonly DebugTarget[], query: string): DebugTarget[] {
    const q = query.trim().toLowerCase();
    if (!q) return [...targets];
    return targets.filter(
        (t) =>
            t.name.toLowerCase().includes(q) ||
            String(t.qq_id).includes(q) ||
            t.bot_id.toLowerCase().includes(q),
    );
}

/** 列表里用的两个字母的后端标记 */
export function backendShortLabel(backend: BackendType): 'NC' | 'SL' {
    return backend === 'snowluma' ? 'SL' : 'NC';
}

/**
 * 进调试台时没有有效的选中 Bot（第一次进、或者上次选的被删了）该默认选谁：
 * 优先在跑的，其次列表第一个；列表空返回 null。
 */
export function defaultTargetId(targets: readonly DebugTarget[]): string | null {
    return (targets.find((t) => t.running) ?? targets[0])?.bot_id ?? null;
}
