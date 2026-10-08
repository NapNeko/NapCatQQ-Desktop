// 发送、重试、撤回、戳一戳、转发的状态唯一所有者是 ChatAccountStore（epoch 守卫、
// 消息状态机、失败回写都在里面），这里只转调它的公开 action，不另起 useMutation
// 造成双份 in-flight。错误原样抛出，调用点保留各自的 errorText / String 措辞。
import { useCallback, useMemo } from 'react';
import { useMutation } from '@tanstack/react-query';
import { chatService } from '../../core/services/chat.service';
import type { ChatAccountStore } from './chatStore';
import type { Contact, SessionKey } from '../../core/domain/chat/model';

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
    // 截图附件的本地预览图读取；与 pickFile/openLink 同类薄转发，不让模块层直连 service。
    const readLocalImage = useCallback((path: string) => chatService.readLocalImage(path), []);
    return useMemo(
        () => ({ send, recall, retry, poke, forward, pickFile, openLink, readLocalImage }),
        [send, recall, retry, poke, forward, pickFile, openLink, readLocalImage],
    );
}
