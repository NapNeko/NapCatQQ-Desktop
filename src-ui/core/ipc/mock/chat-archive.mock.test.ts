import { beforeEach, describe, expect, it } from 'vitest';
import { chatArchiveMock } from './chat-archive.mock';
import type { ChatArchive } from '../generated/chat/ChatArchive';

const archive = (): ChatArchive => ({ v: 1, selfId: '10001', conversations: [], messages: [] });

describe('chat archive preview storage', () => {
    beforeEach(() => localStorage.clear());

    it('restores snapshots and isolates bots and identities', async () => {
        await chatArchiveMock.save('bot-1', '10001', archive());
        expect(await chatArchiveMock.load('bot-1', '10001')).toEqual(archive());
        expect(await chatArchiveMock.load('bot-2', '10001')).toBeNull();
        expect(await chatArchiveMock.load('bot-1', '10002')).toBeNull();
    });

    it('reports unsupported archives without overwriting the file', async () => {
        await chatArchiveMock.save('bot-1', '10001', archive());
        await expect(chatArchiveMock.save('bot-1', '10001', { ...archive(), v: 2 })).rejects.toThrow();
        expect(await chatArchiveMock.load('bot-1', '10001')).toEqual(archive());
    });

    it('keeps errors for damaged stored content visible', async () => {
        await chatArchiveMock.save('bot-1', '10001', archive());
        const key = localStorage.key(0)!;
        localStorage.setItem(key, '{broken');
        await expect(chatArchiveMock.load('bot-1', '10001')).rejects.toThrow();
        await expect(chatArchiveMock.save('bot-1', '10001', archive())).rejects.toThrow();
        expect(localStorage.getItem(key)).toBe('{broken');
    });

    it('does not persist inline attachment data or protocol credentials', async () => {
        const value = archive();
        value.messages.push({ key: 'private:20001/1', session: 'private:20001', senderId: '20001', senderName: '朋友', at: 1, mine: false, status: 'sent', segments: [
            { type: 'text', data: { text: '/help' } },
            { type: 'image', data: { file: 'base64://c2VjcmV0', token: 'secret', nested: { path: 'file://C:/private.png' }, summary: '图片' } },
        ] });
        await chatArchiveMock.save('bot-1', '10001', value);
        const loaded = await chatArchiveMock.load('bot-1', '10001');
        expect(loaded?.messages[0].segments).toEqual([
            { type: 'text', data: { text: '/help' } },
            { type: 'image', data: { nested: {}, summary: '图片' } },
        ]);
        expect(JSON.stringify(localStorage)).not.toContain('base64://');
    });
});
