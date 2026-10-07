import { describe, expect, it, vi } from 'vitest';
import type { DebugCallResponse } from '../ipc/generated/debug/DebugCallResponse';
import type { DebugTarget } from '../ipc/generated/debug/DebugTarget';
import { createFavoriteStickerService } from './favorite-sticker.service';
import { chatMediaService } from './chat-media.service';

const target: DebugTarget = {
    bot_id: 'bot',
    name: '测试',
    qq_id: 99,
    backend: 'napcat',
    host: { kind: 'remote', server_id: 'remote' },
    running: true,
    online: true,
};
const ok = (data: unknown): DebugCallResponse => ({
    request_id: 'req',
    result: {
        kind: 'ok',
        outcome: {
            ok: true,
            data,
            status: 'ok',
            retcode: 0,
            message: '',
            wording: '',
            raw: {},
            channel: { kind: 'internal' },
            elapsed_ms: 1,
            size_bytes: 0,
            truncated: false,
        },
    },
});

describe('favorite sticker protocol', () => {
    it('recovers an evicted image by message and ordinal before adding a Bot host file', async () => {
        const call = vi.fn(async (_bot, action) =>
            action === 'get_msg'
                ? ok({
                      message: [
                          { type: 'image', data: { file: 'first.png' } },
                          { type: 'image', data: { file: 'selected.png' } },
                      ],
                  })
                : action === 'get_image'
                  ? ok({ file: '/bot/cache/selected.png' })
                  : ok(null),
        );
        await createFavoriteStickerService(call).add(
            target,
            { type: 'image', data: { file: 'ncd-inline-image://evicted' } },
            { messageId: '123', imageIndex: 1 },
        );
        expect(call.mock.calls).toEqual([
            ['bot', 'get_msg', { message_id: '123' }],
            ['bot', 'get_image', { file: 'selected.png' }],
            ['bot', 'add_custom_face', { file: '/bot/cache/selected.png' }],
        ]);
    });
    it('adds the resolved NapCat host file and refreshes only that account after actual success', async () => {
        const invalidate = vi.spyOn(chatMediaService, 'invalidateFavorites');
        const call = vi
            .fn()
            .mockResolvedValueOnce(ok({ file: '/bot/cache/face.gif' }))
            .mockResolvedValueOnce(ok(null));
        const service = createFavoriteStickerService(call);
        const refreshed = vi.fn();
        const otherAccount = vi.fn();
        service.subscribe(target, refreshed);
        service.subscribe({ ...target, qq_id: 100 }, otherAccount);
        await service.add(target, { type: 'image', data: { file_id: 'image-id' } });
        expect(call.mock.calls).toEqual([
            ['bot', 'get_image', { file: 'image-id' }],
            ['bot', 'add_custom_face', { file: '/bot/cache/face.gif' }],
        ]);
        expect(refreshed).toHaveBeenCalledOnce();
        expect(otherAccount).not.toHaveBeenCalled();
        expect(invalidate).toHaveBeenCalledWith(target);
        call.mockResolvedValueOnce(ok({ file: '/bot/cache/face.gif' })).mockResolvedValueOnce(
            ok({ result: 1, errMsg: '收藏已满' }),
        );
        await expect(
            service.add(target, { type: 'image', data: { file_id: 'other-image' } }),
        ).rejects.toThrow('收藏已满');
        expect(refreshed).toHaveBeenCalledOnce();
        expect(invalidate).toHaveBeenCalledOnce();
    });
    it('keeps market face metadata carried by an image while resolving only its exact URL', async () => {
        const call = vi
            .fn()
            .mockResolvedValueOnce(ok({ file: 'C:\\bot\\cache\\face.gif' }))
            .mockResolvedValueOnce(ok(null));
        const service = createFavoriteStickerService(call);
        await service.add(target, {
            type: 'image',
            data: {
                file: '0',
                url: 'https://cdn.example/selected.gif',
                emoji_id: '7',
                emoji_package_id: '8',
            },
        });
        expect(call.mock.calls).toEqual([
            ['bot', 'download_file', { url: 'https://cdn.example/selected.gif', thread_count: 1 }],
            [
                'bot',
                'add_custom_face',
                {
                    file: 'C:\\bot\\cache\\face.gif',
                    emoji_id: '7',
                    package_id: '8',
                    is_mark_face: true,
                },
            ],
        ]);
        call.mockClear();
        await expect(
            service.add(target, { type: 'image', data: { local_file: 'C:/desktop/face.gif' } }),
        ).rejects.toThrow('原图片已不可用');
        expect(call).not.toHaveBeenCalled();
    });
    it('uses a SnowLuma source once and leaves a transport timeout uncertain without emitting success', async () => {
        let resolve!: (response: DebugCallResponse) => void;
        const call = vi.fn().mockImplementationOnce(
            () =>
                new Promise<DebugCallResponse>((done) => {
                    resolve = done;
                }),
        );
        const service = createFavoriteStickerService(call);
        const snow = { ...target, backend: 'snowluma' as const };
        const refreshed = vi.fn();
        service.subscribe(snow, refreshed);
        const segment = { type: 'image', data: { url: 'https://cdn.example/face.gif' } };
        const first = service.add(snow, segment);
        await expect(service.add(snow, segment)).rejects.toThrow('正在添加表情');
        expect(call).toHaveBeenCalledWith('bot', 'add_custom_face', { file: segment.data.url });
        resolve({
            request_id: 'req',
            result: { kind: 'err', error: { kind: 'timeout', ms: 30000 } },
        });
        await expect(first).rejects.toMatchObject({ uncertain: true });
        expect(call).toHaveBeenCalledOnce();
        expect(refreshed).not.toHaveBeenCalled();
    });
});
