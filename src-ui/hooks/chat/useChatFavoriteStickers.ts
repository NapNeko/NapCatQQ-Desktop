// 收口 ChatEmojiPicker 的收藏表情列表与收藏变更订阅、ChatMessageActions 的"添加到收藏表情"。
// 列表刷新单点走 favoriteStickerService 订阅(服务 add 成功后自行清缓存并通知监听者),
// mutation 不做二次 invalidate,避免与订阅重复触发请求。
import { useCallback, useEffect } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import type { DebugTarget } from '../../core/ipc/generated/debug/DebugTarget';
import type { Segment } from '../../core/domain/debug/segments';
import type { FavoriteEmoji, ImageSourceContext } from '../../core/domain/chat/media';
import { chatMediaService } from '../../core/services/chat-media.service';
import {
    favoriteStickerService,
    FavoriteStickerError,
} from '../../core/services/favorite-sticker.service';

export { FavoriteStickerError };
export type { FavoriteEmoji };

const favoriteFacesKey = (target: DebugTarget) =>
    ['chat', 'favorite-faces', target.backend, target.bot_id, String(target.qq_id)] as const;

/** enabled 由调用方按"收藏 Tab 打开且可发送"传入;服务自带 60 秒缓存与去重,这里 staleTime 0 忠实复现原挂载即读。 */
export function useFavoriteEmojis(target: DebugTarget, enabled: boolean) {
    const queries = useQueryClient();
    const refresh = useCallback(() => {
        chatMediaService.invalidateFavorites(target);
        void queries.invalidateQueries({ queryKey: favoriteFacesKey(target) });
    }, [target.backend, target.bot_id, target.qq_id, queries]);
    useEffect(
        () =>
            favoriteStickerService.subscribe(target, () => {
                // 与原选择器一致:服务通知到达时清缓存并重读(retry+1 → refresh 重取)。
                refresh();
            }),
        [target.backend, target.bot_id, target.qq_id, refresh],
    );
    const query = useQuery({
        queryKey: favoriteFacesKey(target),
        queryFn: () => chatMediaService.favoriteDetails(target),
        enabled,
        staleTime: 0,
    });
    return {
        favorites: query.data as FavoriteEmoji[] | undefined,
        isLoading: query.isPending,
        error: query.error,
        refresh,
    };
}

/** 调用方保留逐条消息的 pending/unknown 状态与提示文案;catch 里 instanceof FavoriteStickerError 判定不变。 */
export function useAddFavoriteSticker(target: DebugTarget) {
    return useMutation({
        mutationFn: (input: { segment: Segment; context?: ImageSourceContext }) =>
            favoriteStickerService.add(target, input.segment, input.context),
    });
}
