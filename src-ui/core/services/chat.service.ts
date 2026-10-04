// 聊天只使用独立 IPC，不复用调试台的开关或接收器。
import { Channel, invoke, isTauri, pickAnyFiles, openExternalUrl } from '../ipc/transport';
import { chatMock } from '../ipc/mock/chat.mock';
import { localFilesInParams } from '../domain/debug/streamActions';
import type { DebugTarget } from '../ipc/generated/debug/DebugTarget';
import type { DebugEventBatch } from '../ipc/generated/debug/DebugEventBatch';
import type { DebugCallResponse } from '../ipc/generated/debug/DebugCallResponse';
import type { DebugCallRequest } from '../ipc/generated/debug/DebugCallRequest';
import type { DebugSubscribeResponse } from '../ipc/generated/debug/DebugSubscribeResponse';
import type { DebugStreamProgress } from '../ipc/generated/debug/DebugStreamProgress';

export const chatService = {
    targets: (): Promise<DebugTarget[]> => isTauri ? invoke('chat_targets') : chatMock.targets(),
    async call(botId: string, action: string, params: unknown, requestId = crypto.randomUUID()): Promise<DebugCallResponse> {
        const request: DebugCallRequest = { bot_id: botId, request_id: requestId, action, params, channel: { kind: 'auto' }, origin: 'picker', timeout_ms: 30_000 };
        if (!isTauri) return chatMock.call(request);
        const local_files = localFilesInParams(params);
        if (local_files.length) {
            const progress = new Channel<DebugStreamProgress>();
            progress.onmessage = () => {};
            return invoke('chat_call_stream', { request: { ...request, local_files }, progress });
        }
        return invoke('chat_call', { request });
    },
    subscribe(botId: string, onBatch: (batch: DebugEventBatch) => void): Promise<DebugSubscribeResponse> {
        if (!isTauri) return chatMock.subscribe(botId, onBatch);
        const events = new Channel<DebugEventBatch>();
        events.onmessage = onBatch;
        return invoke('chat_subscribe', { botId, events });
    },
    unsubscribe: (subscriptionId: string): Promise<void> => isTauri ? invoke('chat_unsubscribe', { subscriptionId }) : chatMock.unsubscribe(subscriptionId),
    async pickFile(): Promise<{ path: string; name: string } | null> {
        if (!isTauri) return { path: 'preview://design.png', name: '设计稿.png' };
        const [path] = await pickAnyFiles('选择附件');
        return path ? { path, name: path.split(/[\\/]/).pop() || path } : null;
    },
    readLocalImage: (path: string): Promise<string> => isTauri ? invoke('chat_read_local_image', { path }) : Promise.reject(new Error('预览模式不支持读取本机文件')),
    async openLink(url: string): Promise<void> {
        if (!/^https?:\/\//i.test(url)) throw new Error('仅支持打开网页链接');
        if (isTauri) await openExternalUrl(url);
        else window.open(url, '_blank', 'noopener,noreferrer');
    },
};
