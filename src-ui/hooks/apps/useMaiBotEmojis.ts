// 麦麦的表情包：列表、概况、操作、单张图、上传。要麦麦在跑、WebUI 应答了。
// 图单独缓存（按 id 存的文件不会变），不挂在列表的 key 下面：改个标签不该把一页图全重拉一遍。

import { keepPreviousData, useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { maibotResourcesService as svc } from '../../core/services/maibot-resources.service';
import { toAppConfigError } from '../../core/domain/apps/appConfigError';
import { pushAppErrorBar } from './pushAppErrorBar';
import { localFilesOrNothing, useResourceAction } from './maibotResourceAction';
import type {
    MaiBotEmojiAction,
    MaiBotEmojiImage,
    MaiBotEmojiOverview,
    MaiBotEmojiPage,
    MaiBotEmojiQuery,
    MaiBotEmojiUpload,
    MaiBotEmojiUploadDone,
} from '../../core/ipc/types';

const emojiKey = (id: string) => ['maibotEmojis', id] as const;

export function useMaiBotEmojis(instanceId: string, query: MaiBotEmojiQuery, enabled: boolean) {
    return useQuery<MaiBotEmojiPage, Error>({
        queryKey: [...emojiKey(instanceId), 'list', query],
        queryFn: () => svc.emojis(instanceId, query),
        enabled,
        retry: false,
        placeholderData: keepPreviousData,
        staleTime: 15_000,
    });
}

export function useMaiBotEmojiOverview(instanceId: string, enabled: boolean) {
    return useQuery<MaiBotEmojiOverview, Error>({
        queryKey: [...emojiKey(instanceId), 'overview'],
        queryFn: () => svc.emojiOverview(instanceId),
        enabled,
        retry: false,
        staleTime: 30_000,
    });
}

export function useMaiBotEmojiAction(instanceId: string) {
    return useResourceAction<MaiBotEmojiAction>(instanceId, emojiKey(instanceId), svc.emojiAction, '表情包没改成');
}

/** 一张图。缩略图第一次要时上游现生成，后端已经等过几轮，这里再重试两次兜底 */
export function useMaiBotEmojiImage(instanceId: string, emojiId: number, original: boolean, enabled = true) {
    return useQuery<MaiBotEmojiImage, Error>({
        queryKey: ['maibotEmojiImage', instanceId, emojiId, original],
        queryFn: () => svc.emojiImage(instanceId, emojiId, original),
        enabled,
        staleTime: Infinity,
        gcTime: 10 * 60_000,
        retry: 2,
        retryDelay: 1500,
    });
}

const emojiFiles = {
    pick: () => localFilesOrNothing(svc.pickEmojiFiles(), 'maibotEmoji-pick', '打不开选图框'),
    read: (paths: string[]) => localFilesOrNothing(svc.localImages(paths), 'maibotEmoji-read', '读不出这些图'),
};

/** 上传前从系统对话框挑图、看拖进窗口的图 */
export function useMaiBotEmojiFiles() {
    return emojiFiles;
}

export function useMaiBotEmojiUpload(instanceId: string) {
    const qc = useQueryClient();
    return useMutation<MaiBotEmojiUploadDone, unknown, MaiBotEmojiUpload>({
        mutationFn: (up) => svc.emojiUpload(instanceId, up),
        onSuccess: () => void qc.invalidateQueries({ queryKey: emojiKey(instanceId) }),
        onError: (err) => {
            pushAppErrorBar({ key: `maibotEmojiUpload-fail:${instanceId}`, title: '表情包没传上去', raw: toAppConfigError(err).message });
        },
    });
}
