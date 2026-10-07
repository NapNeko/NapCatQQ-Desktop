// 正式环境由 runtime 保存档案，浏览器预览使用独立模拟存储。
import { invoke, isTauri } from '../ipc/transport';
import type { ChatArchive } from '../ipc/generated/chat/ChatArchive';

const previewMock = () =>
    import('../ipc/mock/chat-archive.mock').then((module) => module.chatArchiveMock);

export const chatArchiveService = {
    load: async (botId: string, selfId: string): Promise<ChatArchive | null> =>
        isTauri
            ? invoke('chat_archive_load', { botId, selfId })
            : (await previewMock()).load(botId, selfId),
    save: async (botId: string, selfId: string, archive: ChatArchive): Promise<void> =>
        isTauri
            ? invoke('chat_archive_save', { botId, selfId, archive })
            : (await previewMock()).save(botId, selfId, archive),
};
