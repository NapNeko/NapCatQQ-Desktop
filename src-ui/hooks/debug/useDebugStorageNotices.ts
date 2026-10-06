// 落盘文件（工作区 / 收藏 / 历史）损坏被后端挪走时的提示，页面顶部据此告诉用户原文件去哪了。

import { useQuery } from '@tanstack/react-query';
import { onebotDebugService } from '../../core/services/onebot-debug.service';
import { errorText } from '../../core/domain/errors';
import { pushErrorBar } from '../ui/pushErrorBar';
import type { DebugStorageNotice } from '../../core/ipc/generated/debug/DebugStorageNotice';
import { debugStorageNoticesKey } from './keys';

// 后端这个命令虽会等存储读完，但工作区没读之前发出去的话，触发的那次整盘加载还未必轮到
// 损坏探测 —— 最稳的是等页面把工作区读完（那次加载里三份文件一起读过）再取
export function useDebugStorageNotices({ enabled = true }: { enabled?: boolean } = {}) {
    return useQuery<DebugStorageNotice[], Error>({
        queryKey: debugStorageNoticesKey,
        queryFn: async () => {
            try {
                return await onebotDebugService.storageNotices();
            } catch (err) {
                pushErrorBar({
                    key: 'debug-storage-notices',
                    title: '读取存储提示失败',
                    raw: errorText(err),
                });
                throw err instanceof Error ? err : new Error(errorText(err));
            }
        },
        enabled,
        // 只在后端启动读盘时产生，运行期间不会变
        staleTime: Infinity,
    });
}
