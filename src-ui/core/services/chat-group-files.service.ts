// 群文件：浏览和改动走 runtime 归一好的命令，上传沿用聊天的本机文件通道。
import { Channel, invoke, isTauri, pickAnyFiles, saveFileAs } from '../ipc/transport';
import { chatGroupFilesMock } from '../ipc/mock/chat-group-files.mock';
import { callProblem } from '../domain/debug/errorCopy';
import { localFileTokenFor } from '../domain/debug/streamActions';
import { chatService } from './chat.service';
import type { DebugTarget } from '../ipc/generated/debug/DebugTarget';
import type { DebugStreamProgress } from '../ipc/generated/debug/DebugStreamProgress';
import type { GroupFileListing } from '../ipc/generated/chat/GroupFileListing';
import type { GroupFileSpace } from '../ipc/generated/chat/GroupFileSpace';
import type { GroupFileAction } from '../ipc/generated/chat/GroupFileAction';
import type { ChatFileSource } from '../ipc/generated/chat/ChatFileSource';
import type { ChatFileDownloadRequest } from '../ipc/generated/chat/ChatFileDownloadRequest';

export interface LocalUpload {
    path: string;
    name: string;
}
const errorText = (error: unknown) => (error instanceof Error ? error.message : String(error));
const peer = (target: DebugTarget, value: string) =>
    target.backend === 'snowluma' ? Number(value) : value;

export const chatGroupFilesService = {
    list(
        target: DebugTarget,
        groupId: string,
        folderId: string | null,
        limit?: number,
    ): Promise<GroupFileListing> {
        return isTauri
            ? invoke('chat_group_files', {
                  botId: target.bot_id,
                  groupId,
                  folderId,
                  limit: limit ?? null,
              })
            : chatGroupFilesMock.list(groupId, folderId);
    },
    space(target: DebugTarget, groupId: string): Promise<GroupFileSpace> {
        return isTauri
            ? invoke('chat_group_file_space', { botId: target.bot_id, groupId })
            : chatGroupFilesMock.space(groupId);
    },
    act(target: DebugTarget, groupId: string, action: GroupFileAction): Promise<void> {
        return isTauri
            ? invoke('chat_group_file_act', { botId: target.bot_id, groupId, action })
            : chatGroupFilesMock.act(groupId, action);
    },
    /** 自己在群里的角色；读不到给 null，界面按「不确定」处理 */
    async selfRole(target: DebugTarget, groupId: string): Promise<string | null> {
        try {
            const response = await chatService.call(target.bot_id, 'get_group_member_info', {
                group_id: peer(target, groupId),
                user_id: peer(target, String(target.qq_id)),
                no_cache: false,
            });
            if (response.result.kind !== 'ok' || !response.result.outcome.ok) return null;
            const role = (response.result.outcome.data as { role?: unknown } | null)?.role;
            return typeof role === 'string' && role ? role : null;
        } catch {
            return null;
        }
    },
    /** 先弹另存为，用户取消返回 null；成功返回落盘路径 */
    async download(
        target: DebugTarget,
        source: ChatFileSource,
        name: string,
        requestId: string,
        onProgress: (progress: DebugStreamProgress) => void,
    ): Promise<string | null> {
        if (!isTauri) {
            const size =
                source.kind === 'group'
                    ? chatGroupFilesMock.sizeOf(source.groupId, source.fileId)
                    : 1_000_000;
            await chatGroupFilesMock.transfer(requestId, name, size, 'downloading', onProgress);
            return `C:\\Users\\预览\\Downloads\\${name}`;
        }
        const dest = await saveFileAs('保存文件', name);
        if (!dest) return null;
        const request: ChatFileDownloadRequest = {
            requestId,
            botId: target.bot_id,
            source,
            name,
            dest,
        };
        const progress = new Channel<DebugStreamProgress>();
        progress.onmessage = onProgress;
        return invoke('chat_file_download', { request, progress });
    },
    cancel: (requestId: string): Promise<void> => chatService.cancel(requestId),
    async open(path: string, reveal: boolean): Promise<void> {
        if (!isTauri) return;
        await invoke('chat_open_download', { path, reveal });
    },
    async pickUploads(): Promise<LocalUpload[]> {
        if (!isTauri) return [{ path: 'preview://会议纪要.docx', name: '会议纪要.docx' }];
        const paths = await pickAnyFiles('上传到群文件');
        return paths.map((path) => ({ path, name: path.split(/[\\/]/).pop() || path }));
    },
    /** 上传同时会在群里发一条文件消息，和 QQ 一致 */
    async upload(
        target: DebugTarget,
        groupId: string,
        folderId: string,
        file: LocalUpload,
        requestId: string,
        onProgress: (progress: DebugStreamProgress) => void,
    ): Promise<void> {
        const params: Record<string, unknown> = {
            group_id: peer(target, groupId),
            file: localFileTokenFor(file.path),
            name: file.name,
            upload_file: true,
        };
        if (folderId && folderId !== '/') params.folder_id = folderId;
        let response;
        try {
            response = await chatService.call(
                target.bot_id,
                'upload_group_file',
                params,
                requestId,
                onProgress,
            );
        } catch (error) {
            throw new Error(errorText(error));
        }
        const problem = callProblem(response);
        if (problem) throw new Error(problem);
    },
};
