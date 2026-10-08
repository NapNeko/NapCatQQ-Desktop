// 收口 GroupFilesDialog 的列表/根目录/空间/角色查询与文件整理操作。
// 查询键沿用 ['chat','group-files', scope, groupId, ...] 前缀,操作后 onSettled 整前缀失效,
// 与对话框原本 finally 里 refresh() 的成败都刷新一致;下载/上传转接仍走 fileTransfers。
import { useCallback } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import type { DebugTarget } from '../../core/ipc/generated/debug/DebugTarget';
import { ROOT_FOLDER } from '../../core/domain/chat/groupFiles';
import { chatGroupFilesService } from '../../core/services/chat-group-files.service';
import type { GroupFileAction } from '../../core/ipc/generated/chat/GroupFileAction';
import { scopeOf } from './fileTransfers';

export type { LocalUpload } from '../../core/services/chat-group-files.service';

const filesBaseKey = (target: DebugTarget, groupId: string) =>
    ['chat', 'group-files', scopeOf(target), groupId] as const;

/** 当前目录与移动目标根目录共用:folderId 传 null 即根目录,键位仍按 ROOT_FOLDER 归一。 */
export function useChatGroupFileListing(
    target: DebugTarget,
    groupId: string,
    enabled: boolean,
    folderId: string | null,
    limit: number | undefined,
) {
    return useQuery({
        queryKey: [...filesBaseKey(target, groupId), 'list', folderId ?? ROOT_FOLDER, limit ?? 0],
        queryFn: () => chatGroupFilesService.list(target, groupId, folderId, limit),
        enabled,
        staleTime: 15_000,
    });
}

export function useChatGroupFileSpace(target: DebugTarget, groupId: string, enabled: boolean) {
    return useQuery({
        queryKey: [...filesBaseKey(target, groupId), 'space'],
        queryFn: () => chatGroupFilesService.space(target, groupId),
        enabled,
        staleTime: 30_000,
    });
}

export function useChatGroupFileSelfRole(target: DebugTarget, groupId: string, enabled: boolean) {
    return useQuery({
        queryKey: [...filesBaseKey(target, groupId), 'role'],
        queryFn: () => chatGroupFilesService.selfRole(target, groupId),
        enabled,
        staleTime: 300_000,
    });
}

/** 重命名/移动/删除/建目录/转永久统一入口;成败都刷新整组查询,调用点保留 busy/pending/文案。 */
export function useChatGroupFileAction(target: DebugTarget, groupId: string) {
    const queries = useQueryClient();
    return useMutation({
        mutationFn: (action: GroupFileAction) => chatGroupFilesService.act(target, groupId, action),
        onSettled: () => {
            void queries.invalidateQueries({ queryKey: filesBaseKey(target, groupId) });
        },
    });
}

export function useInvalidateChatGroupFiles(target: DebugTarget, groupId: string): () => void {
    const queries = useQueryClient();
    return useCallback(() => {
        void queries.invalidateQueries({ queryKey: filesBaseKey(target, groupId) });
    }, [target.backend, target.bot_id, target.qq_id, groupId, queries]);
}

export const pickChatGroupUploads = () => chatGroupFilesService.pickUploads();
