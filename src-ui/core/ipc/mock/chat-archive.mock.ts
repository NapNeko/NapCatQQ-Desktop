// 浏览器预览的聊天档案，不参与 Tauri 生产存储。
import type { ChatArchive } from '../generated/chat/ChatArchive';

const storageKey = (botId: string, selfId: string) => `ncd.chat.mock.archive.v1:${JSON.stringify([botId, selfId])}`;
const sensitive = /^(token|access_token|password|authorization|cookie|base64|path|local_path)$/i;
const transient = (value: string) => /^(base64:|data:|file:|ncd-local-file:|blob:|[a-z]:)/i.test(value) || value.startsWith('/') || value.charCodeAt(0) === 92;

function persistedValue(value: unknown): unknown {
    if (typeof value === 'string' && transient(value)) return undefined;
    if (Array.isArray(value)) return value.map(persistedValue).filter(v => v !== undefined);
    if (value && typeof value === 'object') return Object.fromEntries(
        Object.entries(value).filter(([key]) => !sensitive.test(key))
            .map(([key, nested]) => [key, persistedValue(nested)])
            .filter(([, nested]) => nested !== undefined),
    );
    return value;
}

function persistable(archive: ChatArchive): ChatArchive {
    return {
        v: archive.v, selfId: archive.selfId, conversations: archive.conversations,
        messages: archive.messages.map(message => ({
            ...message, segments: message.segments.map(segment => ({
                type: segment.type, data: Object.fromEntries(Object.entries(segment.data).flatMap(([key, value]) => {
                    if (segment.type === 'text' && key === 'text') return [[key, value]];
                    const kept = sensitive.test(key) ? undefined : persistedValue(value);
                    return kept === undefined ? [] : [[key, kept]];
                })),
            })),
        })),
    };
}

function validate(value: unknown, selfId: string): asserts value is ChatArchive {
    if (!value || typeof value !== 'object') throw new Error('聊天档案格式无效');
    const archive = value as Partial<ChatArchive>;
    if (archive.v !== 1 || archive.selfId !== selfId || !Array.isArray(archive.conversations) || !Array.isArray(archive.messages)) throw new Error('聊天档案版本或账号不匹配');
    if (archive.conversations.length > 1000 || archive.messages.length > 5000) throw new Error('聊天档案数量过多');
}

export const chatArchiveMock = {
    async load(botId: string, selfId: string): Promise<ChatArchive | null> {
        const raw = localStorage.getItem(storageKey(botId, selfId));
        if (raw === null) return null;
        const archive: unknown = JSON.parse(raw);
        validate(archive, selfId);
        return archive;
    },
    async save(botId: string, selfId: string, archive: ChatArchive): Promise<void> {
        validate(archive, selfId);
        await chatArchiveMock.load(botId, selfId);
        localStorage.setItem(storageKey(botId, selfId), JSON.stringify(persistable(archive)));
    },
};
