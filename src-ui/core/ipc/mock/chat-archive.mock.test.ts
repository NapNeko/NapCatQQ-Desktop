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
});
