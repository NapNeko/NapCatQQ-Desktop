// 一个 Bot 的通道列表和连通测试。

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { onebotDebugService } from '../../core/services/onebot-debug.service';
import { errorText } from '../../core/domain/errors';
import { pushErrorBar } from '../ui/pushErrorBar';
import type { DebugChannelId } from '../../core/ipc/generated/debug/DebugChannelId';
import type { DebugChannelInfo } from '../../core/ipc/generated/debug/DebugChannelInfo';
import type { DebugChannels } from '../../core/ipc/generated/debug/DebugChannels';
import { channelIdKey, debugChannelsKey, debugIdleKey } from './keys';

export function useDebugChannels(botId: string | null) {
    return useQuery<DebugChannels, Error>({
        queryKey: botId ? debugChannelsKey(botId) : debugIdleKey,
        queryFn: async () => {
            if (!botId) throw new Error('未选择 Bot');
            try {
                return await onebotDebugService.channels(botId);
            } catch (err) {
                pushErrorBar({
                    key: `debug-channels:${botId}`,
                    title: '读取通道列表失败',
                    raw: errorText(err),
                });
                throw err instanceof Error ? err : new Error(errorText(err));
            }
        },
        enabled: !!botId,
        // 通道状态由 Bot 状态 / WebUI / 登录事件失效；这里只是兜底
        staleTime: 15_000,
    });
}

/** 测一条通道，把测出来的状态直接写回该通道在缓存里的那一项 */
export function useTestChannel() {
    const client = useQueryClient();
    return useMutation<DebugChannelInfo, unknown, { botId: string; channel: DebugChannelId }>({
        mutationFn: ({ botId, channel }) => onebotDebugService.testChannel(botId, channel),
        onSuccess: (info, { botId, channel }) => {
            const key = channelIdKey(channel);
            client.setQueryData<DebugChannels>(debugChannelsKey(botId), (old) =>
                old
                    ? {
                          ...old,
                          channels: old.channels.map((c) =>
                              channelIdKey(c.id) === key ? info : c,
                          ),
                      }
                    : old,
            );
        },
        // setQueryData 只换了被测那一行：「自动」落点和别的通道的可达性也可能因为这次测试重算
        onSettled: (_info, _err, { botId }) => {
            void client.invalidateQueries({ queryKey: debugChannelsKey(botId) });
        },
        onError: (err, { botId, channel }) => {
            pushErrorBar({
                key: `debug-test-channel:${botId}:${channelIdKey(channel)}`,
                title: '连通测试失败',
                raw: errorText(err),
            });
        },
    });
}
