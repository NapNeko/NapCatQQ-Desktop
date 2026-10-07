// 发送、重试、撤回、戳一戳、转发的状态唯一所有者是 ChatAccountStore（epoch 守卫、
// 消息状态机、失败回写都在里面），这里只转调它的公开 action，不另起 useMutation
// 造成双份 in-flight。错误原样抛出，调用点保留各自的 errorText / String 措辞。
import { useCallback, useMemo } from 'react';
import { useMutation } from '@tanstack/react-query';
import { chatService } from '../../core/services/chat.service';
import { chatProfileService } from '../../core/services/chat-profile.service';
import type { ChatAccountStore } from './chatStore';
import type { Contact, SessionKey } from '../../core/domain/chat/model';
import type { DebugTarget } from '../../core/ipc/generated/debug/DebugTarget';

export function useChatSend(store: ChatAccountStore) {
    const send = useCallback((key: SessionKey) => store.send(key), [store]);
    const recall = useCallback((messageKey: string) => store.recall(messageKey), [store]);
    const retry = useCallback((messageKey: string) => store.retry(messageKey), [store]);
    const poke = useCallback((key: SessionKey, userId: string) => store.poke(key, userId), [store]);
    const forward = useCallback(
        (messageKeys: readonly string[], contact: Contact) => store.forward(messageKeys, contact),
        [store],
    );
    // 附件选择是发给后端的取文件命令；群文件的真实上传已在 hooks/chat/fileTransfers
    // 里有各自的进度流，这里不重复承载。
    const { mutateAsync } = useMutation({
        mutationFn: () => chatService.pickFile(),
    });
    const pickFile = useCallback(() => mutateAsync(), [mutateAsync]);
    const openLink = useCallback((url: string) => chatService.openLink(url), []);
    return useMemo(
        () => ({ send, recall, retry, poke, forward, pickFile, openLink }),
        [send, recall, retry, poke, forward, pickFile, openLink],
    );
}

// 群管理动作（禁言/移出）没有 store 语义，上游结果不明确的判断（token 守卫、
// 成功文案、onApplied 失效）留在调用方，这里只保证错误原样拒绝。
export function useChatMemberOps() {
    const ban = useMutation({
        mutationFn: (args: [DebugTarget, string, string, number]) =>
            chatProfileService.ban(...args),
    });
    const kick = useMutation({
        mutationFn: (args: [DebugTarget, string, string]) => chatProfileService.kick(...args),
    });
    const { mutateAsync: banAsync } = ban;
    const { mutateAsync: kickAsync } = kick;
    const banMember = useCallback(
        (target: DebugTarget, groupId: string, memberId: string, duration: number) =>
            banAsync([target, groupId, memberId, duration]),
        [banAsync],
    );
    const kickMember = useCallback(
        (target: DebugTarget, groupId: string, memberId: string) =>
            kickAsync([target, groupId, memberId]),
        [kickAsync],
    );
    return useMemo(() => ({ banMember, kickMember }), [banMember, kickMember]);
}
