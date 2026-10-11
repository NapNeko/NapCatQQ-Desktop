import { beforeEach, describe, expect, it, vi } from 'vitest';
import { copyChatMessage } from './chat-clipboard.service';
import { chatMediaService } from './chat-media.service';
import { invoke } from '../ipc/transport';
import type { Message } from '../domain/chat/model';
import type { DebugTarget } from '../ipc/generated/debug/DebugTarget';

vi.mock('../ipc/transport', async (original) => ({
    ...(await original<object>()),
    isTauri: true,
    invoke: vi.fn(),
}));
const target: DebugTarget = {
    bot_id: 'bot',
    qq_id: 99,
    name: '测试',
    backend: 'napcat',
    host: { kind: 'local' },
    running: true,
    online: true,
};
const contact = { key: 'private:12' as const, type: 'private' as const, id: '12', name: '好友' };
const message = (segments: Message['segments']): Message => ({
    key: 'm',
    id: '7',
    session: contact.key,
    senderId: '12',
    senderName: '好友',
    at: 1,
    mine: false,
    status: 'sent',
    segments,
});
beforeEach(() => vi.clearAllMocks());
describe('message clipboard formats', () => {
    it('copies full text and structured content without preview truncation', async () => {
        const xml = `<card>${'完整内容'.repeat(100)}</card>`;
        await copyChatMessage(
            target,
            contact,
            message([
                { type: 'text', data: { text: '  首行\n尾行  ' } },
                { type: 'xml', data: { data: xml } },
            ]),
        );
        expect(invoke).toHaveBeenCalledWith('chat_copy_message', {
            botId: 'bot',
            text: `  首行\n尾行  ${xml}`,
            media: [],
        });
    });
    it('copies image bytes and arbitrary files in the same clipboard transaction', async () => {
        vi.spyOn(chatMediaService, 'image').mockResolvedValue('data:image/png;base64,cG5n');
        await copyChatMessage(
            target,
            contact,
            message([
                { type: 'image', data: { file: 'image-id' } },
                { type: 'file', data: { name: '备份.7z', file_id: 'file-id' } },
                { type: 'text', data: { text: '说明' } },
            ]),
        );
        expect(invoke).toHaveBeenCalledWith(
            'chat_copy_message',
            expect.objectContaining({
                media: [
                    { name: '图片.png', image: true, base64: 'cG5n' },
                    {
                        name: '备份.7z',
                        image: false,
                        source: {
                            kind: 'private',
                            userId: '12',
                            fileId: 'file-id',
                            fileHash: null,
                        },
                    },
                ],
            }),
        );
    });
    it('keeps the clipboard unchanged when media cannot be resolved', async () => {
        vi.spyOn(chatMediaService, 'image').mockRejectedValue(new Error('图片已过期'));
        await expect(
            copyChatMessage(
                target,
                contact,
                message([{ type: 'image', data: { file: 'missing' } }]),
            ),
        ).rejects.toThrow('图片已过期');
        expect(invoke).not.toHaveBeenCalled();
    });
});

it('copies the contents of merged records including embedded images', async () => {
    vi.spyOn(chatMediaService, 'forward').mockResolvedValue([
        {
            senderId: '12',
            name: '好友',
            segments: [
                { type: 'text', data: { text: '记录正文' } },
                { type: 'image', data: { file: 'image-id' } },
            ],
        },
    ]);
    vi.spyOn(chatMediaService, 'image').mockResolvedValue('data:image/png;base64,cG5n');
    await copyChatMessage(
        target,
        contact,
        message([{ type: 'forward', data: { id: 'record-id' } }]),
    );
    expect(invoke).toHaveBeenCalledWith(
        'chat_copy_message',
        expect.objectContaining({
            text: '\n好友：记录正文[图片]',
            media: [{ name: '图片.png', image: true, base64: 'cG5n' }],
        }),
    );
});

it('materializes a local blob preview before writing the Windows clipboard', async () => {
    vi.spyOn(chatMediaService, 'image').mockResolvedValue('blob:local-image');
    vi.spyOn(globalThis, 'fetch').mockResolvedValue({
        ok: true,
        blob: async () => new Blob(['png'], { type: 'image/png' }),
    } as Response);
    await copyChatMessage(
        target,
        contact,
        message([{ type: 'image', data: { inline_ref: 'local-reference' } }]),
    );
    expect(invoke).toHaveBeenCalledWith(
        'chat_copy_message',
        expect.objectContaining({ media: [{ name: '图片.png', image: true, base64: 'cG5n' }] }),
    );
});
