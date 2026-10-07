// 收口 ChatPage 的自身群权限订阅、GroupMembersDialog 的 self 订阅、ChatGroupMemberMenu 的
// warm/清理/失效。权限判定永远只有一份状态:订阅走既有 useGroupMemberPermission(单例 peek +
// useSyncExternalStore),动作后的清理只是对同一单例的转发。
import { useEffect } from 'react';
import type { DebugTarget } from '../../core/ipc/generated/debug/DebugTarget';
import { groupMemberPermissions } from '../../core/services/group-member-permissions.service';
import type { ProfileMember } from '../../core/domain/chat/profile';
import { useGroupMemberPermission } from './useGroupMemberPermission';

export { useGroupMemberPermission };
export type { MemberPermission } from '../../core/domain/chat/profile';

/**
 * ChatPage / GroupMembersDialog 的"我在这个群是谁"订阅。
 * groupId 传 null 表示当前会话不是群或已断开:清缓存并返回 undefined,复现原页面的清理分支。
 * 卸载/切群时的 clear 与原实现相同,只在作用域匹配时整单清理,不会误伤其他群。
 */
export function useChatGroupSelf(target: DebugTarget, groupId: string | null, enabled: boolean) {
    const selfId = String(target.qq_id);
    const active = enabled && !!groupId;
    const permission = useGroupMemberPermission(target, groupId ?? '', selfId, active);
    useEffect(() => {
        if (!active || !groupId) {
            if (groupId) groupMemberPermissions.clear(target, groupId);
            else groupMemberPermissions.clear();
            return;
        }
        void groupMemberPermissions.self(target, groupId);
        return () => {
            groupMemberPermissions.clear(target, groupId);
        };
    }, [target.bot_id, target.qq_id, target.backend, groupId, enabled]);
    return permission;
}

export function warmGroupMemberPermission(
    target: DebugTarget,
    groupId: string,
    memberId: string,
): Promise<ProfileMember | undefined> {
    return groupMemberPermissions.warm(target, groupId, memberId);
}

export function invalidateGroupMemberPermission(
    target: DebugTarget,
    groupId: string,
    memberId: string,
): void {
    groupMemberPermissions.invalidate(target, groupId, memberId);
}

export function clearGroupMemberPermissions(target?: DebugTarget, groupId?: string): void {
    groupMemberPermissions.clear(target, groupId);
}
