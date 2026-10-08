// 收口 ChatDetails 的资料弹层、GroupMembersDialog 的成员列表与详情、ChatMemberActions 的禁言/移出。
// 成员数据到达后向权限单例 seed,保持对话框原本"list 成功即喂权限缓存"的链路不转移给组件。
import { useCallback } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import type { DebugTarget } from '../../core/ipc/generated/debug/DebugTarget';
import type { Contact } from '../../core/domain/chat/model';
import { chatProfileService } from '../../core/services/chat-profile.service';
import { groupMemberPermissions } from '../../core/services/group-member-permissions.service';

export type { ChatProfile, ProfileMember } from '../../core/domain/chat/profile';

const accountKey = (target: DebugTarget) =>
    [target.backend, target.bot_id, String(target.qq_id)] as const;

/** 每次打开弹层都重新读资料(原实现 attempt 即重取),staleTime 0 保持该时机。 */
export function useChatProfile(
    target: DebugTarget | undefined,
    contact: Contact,
    enabled: boolean,
) {
    const query = useQuery({
        queryKey: [
            'chat',
            'profile',
            target?.backend,
            target?.bot_id,
            target ? String(target.qq_id) : undefined,
            contact.type,
            contact.id,
        ],
        queryFn: () => chatProfileService.info(target as DebugTarget, contact),
        enabled: enabled && !!target,
        staleTime: 0,
    });
    return {
        profile: query.data,
        isLoading: query.isPending,
        error: query.error,
        refresh: query.refetch,
    };
}

/** queryFn 里顺带 self() 激活权限单例作用域;seed 只认当前作用域,故与对话框原 Promise.allSettled 语义一致。 */
export function useChatGroupMembers(target: DebugTarget, groupId: string, enabled: boolean) {
    const query = useQuery({
        queryKey: ['chat', 'group-members', ...accountKey(target), groupId],
        queryFn: async () => {
            const [list] = await Promise.allSettled([
                chatProfileService.members(target, groupId),
                groupMemberPermissions.self(target, groupId),
            ]);
            if (list.status !== 'fulfilled') throw list.reason;
            groupMemberPermissions.seed(target, groupId, list.value);
            return list.value;
        },
        enabled,
        staleTime: 0,
    });
    return {
        members: query.data,
        isLoading: query.isPending,
        isFetching: query.isFetching,
        error: query.error,
        refresh: query.refetch,
    };
}

const memberDetailKey = (target: DebugTarget, groupId: string, memberId: string) =>
    ['chat', 'group-member', ...accountKey(target), groupId, memberId] as const;

export function useChatGroupMemberDetail(
    target: DebugTarget,
    groupId: string,
    memberId: string | undefined,
    enabled: boolean,
) {
    const id = memberId ?? '';
    const query = useQuery({
        queryKey: memberDetailKey(target, groupId, id),
        queryFn: async () => {
            const member = await chatProfileService.member(target, groupId, id);
            groupMemberPermissions.seed(target, groupId, [member]);
            return member;
        },
        enabled: enabled && !!memberId,
        staleTime: 0,
    });
    return {
        member: query.data,
        isLoading: query.isPending,
        isFetching: query.isFetching,
        error: query.error,
        refresh: query.refetch,
    };
}

/** 生效后清权限缓存条目并刷新详情查询;调用点自己的本地移除(kick 后列表剔除)与文案保持不变。 */
export function useChatMemberModeration(target: DebugTarget, groupId: string, memberId: string) {
    const queries = useQueryClient();
    const afterApplied = useCallback(() => {
        groupMemberPermissions.invalidate(target, groupId, memberId);
        void queries.invalidateQueries({ queryKey: memberDetailKey(target, groupId, memberId) });
    }, [target.bot_id, target.qq_id, target.backend, groupId, memberId, queries]);
    const ban = useMutation({
        mutationFn: (duration: number) =>
            chatProfileService.ban(target, groupId, memberId, duration),
        onSuccess: afterApplied,
    });
    const kick = useMutation({
        mutationFn: () => chatProfileService.kick(target, groupId, memberId),
        onSuccess: afterApplied,
    });
    return { ban, kick };
}
