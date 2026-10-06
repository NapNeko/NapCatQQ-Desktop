// 收藏与文件夹：读、保存（乐观更新）、导出到文件、从文件导入。

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { onebotDebugService } from '../../core/services/onebot-debug.service';
import { errorText } from '../../core/domain/errors';
import { pushErrorBar } from '../ui/pushErrorBar';
import { pushInfoBar } from '../ui/globalInfoBarStore';
import type { DebugCollections } from '../../core/ipc/generated/debug/DebugCollections';
import { debugCollectionsKey, debugCollectionsSaveKey } from './keys';

export function useDebugCollections() {
    return useQuery<DebugCollections, Error>({
        queryKey: debugCollectionsKey,
        queryFn: async () => {
            try {
                return await onebotDebugService.collections();
            } catch (err) {
                pushErrorBar({
                    key: 'debug-collections',
                    title: '读取收藏失败',
                    raw: errorText(err),
                });
                throw err instanceof Error ? err : new Error(errorText(err));
            }
        },
        // 收藏只有这里在改，写进去的同时已经更新了缓存，不必自动重拉
        staleTime: Infinity,
    });
}

/**
 * 整份替换保存。先把缓存改成新值（界面立刻跟手），后端写失败再处理：
 * - 只有缓存里还是这次的乐观值时才退回去；连着保存时后面那次已经把缓存改成更新的值，
 *   前一次失败不能把它盖回旧的。
 * - 重拉磁盘内容只在最后一次保存结束时做（此时没有别的保存还在排队）。收藏是整份替换，
 *   最后落盘的那份就是磁盘上的真实状态，重拉一次即可收敛；中间几次结束就重拉，
 *   会拿到别的保存还没写完的旧内容，把界面拉回去。
 * 同一个 scope 的保存按发起顺序一个接一个写，后写的不会被先写的覆盖。
 */
export function useSaveCollections() {
    const client = useQueryClient();
    return useMutation<
        void,
        unknown,
        DebugCollections,
        { previous: DebugCollections | undefined; optimistic: DebugCollections | undefined }
    >({
        mutationKey: debugCollectionsSaveKey,
        scope: { id: 'debug-collections-save' },
        mutationFn: (next) => onebotDebugService.saveCollections(next),
        onMutate: async (next) => {
            await client.cancelQueries({ queryKey: debugCollectionsKey });
            const previous = client.getQueryData<DebugCollections>(debugCollectionsKey);
            // 用写进缓存后的引用做「还是不是我的值」的记号：结构共享可能让它不是传进去的那个对象
            const optimistic = client.setQueryData<DebugCollections>(debugCollectionsKey, next);
            return { previous, optimistic };
        },
        onError: (err, _next, context) => {
            if (
                context?.previous &&
                client.getQueryData<DebugCollections>(debugCollectionsKey) === context.optimistic
            ) {
                client.setQueryData<DebugCollections>(debugCollectionsKey, context.previous);
            }
            pushErrorBar({
                key: 'debug-collections-save',
                title: '保存收藏失败',
                raw: errorText(err),
            });
        },
        onSettled: () => {
            // 此刻这次保存自己还算在「进行中」里，所以 <= 1 就是没有别的保存了
            if (client.isMutating({ mutationKey: debugCollectionsSaveKey }) <= 1) {
                void client.invalidateQueries({ queryKey: debugCollectionsKey });
            }
        },
    });
}

/** 弹系统「另存为」选位置，再把收藏写过去；取消选择时什么也不做（返回 null） */
export function useExportCollections() {
    return useMutation<string | null, unknown, void>({
        mutationFn: async () => {
            const path = await onebotDebugService.saveCollectionsFile();
            if (!path) return null;
            await onebotDebugService.exportCollections(path);
            return path;
        },
        onSuccess: (path) => {
            if (path) {
                pushInfoBar({
                    key: 'debug-collections-export',
                    tone: 'success',
                    title: '收藏已导出',
                    content: path,
                    autoDismissMs: 4000,
                });
            }
        },
        onError: (err) => {
            pushErrorBar({
                key: 'debug-collections-export',
                title: '导出收藏失败',
                raw: errorText(err),
            });
        },
    });
}

/**
 * 弹系统对话框选文件，并进现有收藏（id 撞了的后端会改名）；取消选择返回 null。
 * 合并结果以磁盘为准，成功后重拉一次。
 */
export function useImportCollections() {
    const client = useQueryClient();
    return useMutation<DebugCollections | null, unknown, void>({
        mutationFn: async () => {
            const picked = await onebotDebugService.pickCollectionsFile();
            if (!picked) return null;
            if (picked.ignored > 0) {
                pushInfoBar({
                    key: 'debug-import-multi',
                    tone: 'warning',
                    title: '一次只能导入一个文件',
                    content: `选了 ${picked.ignored + 1} 个文件，只导入了第一个，其余 ${picked.ignored} 个没有处理。`,
                    autoDismissMs: 6000,
                });
            }
            return onebotDebugService.importCollections(picked.path);
        },
        onSuccess: (merged) => {
            if (!merged) return;
            void client.invalidateQueries({ queryKey: debugCollectionsKey });
            pushInfoBar({
                key: 'debug-collections-import',
                tone: 'success',
                title: '收藏已导入',
                content: `现在共有 ${merged.requests.length} 个请求、${merged.folders.length} 个文件夹。`,
                autoDismissMs: 4000,
            });
        },
        onError: (err) => {
            pushErrorBar({
                key: 'debug-collections-import',
                title: '导入收藏失败',
                raw: errorText(err),
            });
        },
    });
}
