// 调试台 react-query 缓存键。
//
// 目录和动作说明按 Bot 分键：同一个 Bot 上线 / 下线后要一起失效，靠的是前缀 ['debug', 'catalog', botId]。
// 没有选 Bot 时（只看内置快照）botId 用 null，和真实 Bot 的键不会撞。

import type { BackendType } from '../../core/ipc/generated/domain/BackendType';
import type { DebugChannelId } from '../../core/ipc/generated/debug/DebugChannelId';
import type { DebugHistoryQuery } from '../../core/ipc/generated/debug/DebugHistoryQuery';

export const debugTargetsKey = ['debug', 'targets'] as const;

export const debugChannelsKey = (botId: string) => ['debug', 'channels', botId] as const;

/** 没有可查的目标时挂在这个键上（配合 enabled: false），免得拿一个假 Bot 的键去占缓存 */
export const debugIdleKey = ['debug', 'idle'] as const;

export const debugCatalogKey = (botId: string | null, backend: BackendType) =>
    ['debug', 'catalog', botId, backend] as const;

export const debugCatalogPrefix = (botId: string | null) => ['debug', 'catalog', botId] as const;

export const debugSpecKey = (botId: string | null, backend: BackendType, name: string) =>
    ['debug', 'spec', botId, backend, name] as const;

export const debugSpecPrefix = (botId: string | null) => ['debug', 'spec', botId] as const;

export const debugHistoryPrefix = ['debug', 'history'] as const;

export const debugHistoryKey = (query: DebugHistoryQuery) => ['debug', 'history', query] as const;

export const debugHistoryEntryPrefix = ['debug', 'history-entry'] as const;

export const debugHistoryEntryKey = (id: string) => ['debug', 'history-entry', id] as const;

export const debugCollectionsKey = ['debug', 'collections'] as const;

/** 保存收藏这个 mutation 的键：按它数还有几次保存没结束 */
export const debugCollectionsSaveKey = ['debug', 'collections-save'] as const;

export const debugReceiversKey = ['debug', 'receivers'] as const;

export const debugStorageNoticesKey = ['debug', 'storage-notices'] as const;

export const debugContactsKey = (
    botId: string,
    kind: 'group' | 'friend' | 'member',
    groupId?: string | number | null,
) => ['debug', 'contacts', botId, kind, groupId ?? null] as const;

/** 通道 id 的字符串形式，做比较和 map 键用（后端也是把它当 map 键） */
export function channelIdKey(id: DebugChannelId): string {
    return id.kind === 'http' || id.kind === 'ws' ? `${id.kind}:${id.name}` : id.kind;
}
