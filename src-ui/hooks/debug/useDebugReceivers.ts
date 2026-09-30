// 顶栏「正在接收：N 个 Bot」：后端各接收器的状态，挂载期间每 5 秒刷新一次。

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { onebotDebugService } from '../../core/services/onebot-debug.service';
import { errorText } from '../../core/domain/errors';
import { pushErrorBar } from '../ui/pushErrorBar';
import type { DebugReceiverInfo } from '../../core/ipc/generated/debug/DebugReceiverInfo';
import { debugEventStore } from './debugEventStore';
import { debugReceiversKey } from './keys';

const REFRESH_MS = 5000;

export function useDebugReceivers() {
    return useQuery<DebugReceiverInfo[], Error>({
        queryKey: debugReceiversKey,
        queryFn: async () => {
            try {
                return await onebotDebugService.receivers();
            } catch (err) {
                // 同 key 顶替：轮询反复失败也只留一条
                pushErrorBar({ key: 'debug-receivers', title: '读取事件接收状态失败', raw: errorText(err) });
                throw err instanceof Error ? err : new Error(errorText(err));
            }
        },
        // 只在有组件挂着时轮询；窗口在后台时 react-query 默认暂停
        refetchInterval: REFRESH_MS,
        staleTime: 0,
    });
}

/** 顶栏逐个「停止」：走事件 store（连本地的订阅一起收掉），完事让接收器列表马上刷新 */
export function useStopReceiver() {
    const client = useQueryClient();
    return useMutation<void, unknown, string>({
        mutationFn: (botId) => debugEventStore.stopReceiving(botId),
        onSettled: () => {
            void client.invalidateQueries({ queryKey: debugReceiversKey });
        },
    });
}
