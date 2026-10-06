// 正式环境由 runtime 保存档案，浏览器预览使用独立模拟存储。
import { invoke, isTauri } from '../ipc/transport';
import { chatArchiveMock } from '../ipc/mock/chat-archive.mock';
import type { ChatArchive } from '../ipc/generated/chat/ChatArchive';

export const chatArchiveService = {
    load: (botId: string, selfId: string): Promise<ChatArchive | null> =>
        isTauri
            ? invoke('chat_archive_load', { botId, selfId })
            : chatArchiveMock.load(botId, selfId),
    save: (botId: string, selfId: string, archive: ChatArchive): Promise<void> =>
        isTauri
            ? invoke('chat_archive_save', { botId, selfId, archive })
            : chatArchiveMock.save(botId, selfId, archive),
};
