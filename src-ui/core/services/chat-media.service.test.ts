import { describe, expect, it, vi } from 'vitest';
import { chatService } from './chat.service';
import { chatMediaService, createChatMediaService } from './chat-media.service';
import type { DebugCallResponse } from '../ipc/generated/debug/DebugCallResponse';
import type { DebugTarget } from '../ipc/generated/debug/DebugTarget';
vi.mock('../ipc/transport', async (original) => ({
    ...(await original<typeof import('../ipc/transport')>()),
    isTauri: true,
}));
vi.mock('@tauri-apps/api/core', async (original) => ({
    ...(await original<typeof import('@tauri-apps/api/core')>()),
    convertFileSrc: (path: string) => `asset://localhost/${path}`,
}));
const target: DebugTarget = {
    bot_id: 'bot',
    name: '测试',
    qq_id: 99,
    backend: 'napcat',
    host: { kind: 'local' },
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
describe('native chat media protocol', () => {
    it('uses an injected debug caller without opening a native chat transport', async () => {
        const native = vi.spyOn(chatService, 'call');
        const debug = vi.fn().mockResolvedValue(ok({ url: 'https://cdn.example/debug.png' }));
        await expect(
            createChatMediaService(debug).image(target, { file: 'image-id' }),
        ).resolves.toBe('https://cdn.example/debug.png');
        expect(debug).toHaveBeenCalledWith('bot', 'get_image', { file: 'image-id' });
        expect(native).not.toHaveBeenCalled();
    });
    it.each(['napcat', 'snowluma'] as const)(
        'reads %s forwards using its resource identifier parameter',
        async (backend) => {
            const call = vi.spyOn(chatService, 'call').mockResolvedValue(
                ok({
                    messages: [
                        {
                            sender: { user_id: 123, nickname: '小明' },
                            time: 1700000000,
                            message: [
                                { type: 'text', data: { text: '内容' } },
                                { type: 'forward', data: { id: 'nested' } },
                            ],
                        },
                    ],
                }),
            );
            const nodes = await chatMediaService.forward(
                { ...target, backend },
                { id: 'resource-id' },
            );
            expect(call).toHaveBeenCalledWith(
                'bot',
                'get_forward_msg',
                backend === 'napcat' ? { message_id: 'resource-id' } : { id: 'resource-id' },
            );
            expect(nodes).toEqual([
                {
                    senderId: '123',
                    name: '小明',
                    time: 1700000000000,
                    segments: [
                        { type: 'text', data: { text: '内容' } },
                        { type: 'forward', data: { id: 'nested' } },
                    ],
                },
            ]);
        },
    );
    it('normalizes inline node content without another protocol request', async () => {
        const call = vi.spyOn(chatService, 'call');
        const nodes = await chatMediaService.forward(target, {
            content: [
                {
                    type: 'node',
                    data: { user_id: '123', nickname: '小明', content: '[CQ:face,id=14]你好' },
                },
            ],
        });
        expect(nodes[0]).toMatchObject({
            senderId: '123',
            name: '小明',
            segments: [
                { type: 'face', data: { id: '14' } },
                { type: 'text', data: { text: '你好' } },
            ],
        });
        expect(call).not.toHaveBeenCalled();
    });
    it('requests playable audio and ignores an upstream local output path', async () => {
        const call = vi.spyOn(chatService, 'call').mockResolvedValue(
            ok({
                file: 'C:\private\voice.mp3',
                url: 'C:\private\voice.mp3',
                base64: 'SUQzAA==',
            }),
        );
        expect(
            await chatMediaService.record(target, {
                file: 'voice.silk',
                url: 'https://cdn.example/voice',
            }),
        ).toBe('data:audio/mpeg;base64,SUQzAA==');
        expect(call).toHaveBeenCalledWith('bot', 'get_record', {
            file: 'voice.silk',
            out_format: 'mp3',
        });
    });
    it('requests QQ voice transcription by message id', async () => {
        const call = vi
            .spyOn(chatService, 'call')
            .mockResolvedValue(ok({ text: '  你好，世界  ' }));
        await expect(chatMediaService.transcript(target, '12345')).resolves.toBe('你好，世界');
        expect(call).toHaveBeenCalledWith('bot', 'fetch_ptt_text', { message_id: '12345' });
    });
    it('refreshes an image URL through get_image instead of reusing an expired rkey', async () => {
        const call = vi
            .spyOn(chatService, 'call')
            .mockResolvedValue(ok({ url: 'https://cdn.example/image.png?rkey=fresh' }));
        await expect(
            chatMediaService.image(
                target,
                { file: 'https://cdn.example/image.png?rkey=expired' },
                true,
            ),
        ).resolves.toBe('https://cdn.example/image.png?rkey=fresh');
        expect(call).toHaveBeenCalledWith('bot', 'get_image', {
            file: 'https://cdn.example/image.png?rkey=expired',
        });
    });
    it('accepts base64:// audio returned by get_record', async () => {
        vi.spyOn(chatService, 'call').mockResolvedValue(
            ok({ file: 'base64://UklGRg==', out_format: 'wav' }),
        );
        await expect(chatMediaService.record(target, { file: 'voice.silk' })).resolves.toBe(
            'data:audio/wav;base64,UklGRg==',
        );
    });
    it('does not return the known broken image URL when refresh only returns a host path', async () => {
        vi.spyOn(chatService, 'call').mockResolvedValue(ok({ file: '/tmp/private/image.png' }));
        await expect(
            chatMediaService.image(target, { url: 'https://cdn.example/broken.png' }, true),
        ).rejects.toThrow('可播放图片地址');
    });
    it('tries the file token if get_image does not recognize the file id', async () => {
        const call = vi
            .fn()
            .mockRejectedValueOnce(new Error('未知标识'))
            .mockResolvedValueOnce(ok({ url: 'https://cdn.example/recovered.png' }));
        await expect(
            createChatMediaService(call).image(
                target,
                { file_id: 'opaque-id', file: 'image-token' },
                true,
            ),
        ).resolves.toBe('https://cdn.example/recovered.png');
        expect(call).toHaveBeenNthCalledWith(2, 'bot', 'get_image', { file: 'image-token' });
    });
    it('prefers returned image bytes to a potentially expired address and rejects malformed bytes', async () => {
        const call = vi
            .fn()
            .mockResolvedValueOnce(ok({ url: 'https://cdn.example/old.png', base64: 'R0lGODlh' }))
            .mockResolvedValueOnce(ok({ base64: 'abc' }));
        const service = createChatMediaService(call);
        await expect(service.image(target, { file: 'gif' }, true)).resolves.toBe(
            'data:image/gif;base64,R0lGODlh',
        );
        await expect(service.image(target, { file: 'bad' }, true)).rejects.toThrow(
            '图片内容不完整',
        );
    });
    it('deduplicates concurrent image refreshes without caching a failed request', async () => {
        let reject!: (error: Error) => void;
        const call = vi
            .fn()
            .mockImplementationOnce(
                () =>
                    new Promise((_, fail) => {
                        reject = fail;
                    }),
            )
            .mockResolvedValue(ok({ url: 'https://cdn.example/fresh.png' }));
        const service = createChatMediaService(call);
        const first = service.image(target, { file: 'dedup-image' }, true);
        const second = service.image(target, { file: 'dedup-image' }, true);
        reject(new Error('网络波动'));
        const result = await Promise.allSettled([first, second]);
        expect(result.every((value) => value.status === 'rejected')).toBe(true);
        expect(call).toHaveBeenCalledOnce();
        await expect(service.image(target, { file: 'dedup-image' }, true)).resolves.toBe(
            'https://cdn.example/fresh.png',
        );
    });
    it('limits image resolution to four requests while audio can still start', async () => {
        let active = 0;
        let peak = 0;
        const finish: Array<() => void> = [];
        const call = vi.fn((_bot, action, params) => {
            if (action !== 'get_image') return Promise.resolve(ok({ base64: 'SUQzAA==' }));
            active += 1;
            peak = Math.max(peak, active);
            return new Promise<DebugCallResponse>((resolve) =>
                finish.push(() => {
                    active -= 1;
                    resolve(ok({ url: `https://cdn.example/${params.file}.png` }));
                }),
            );
        });
        const service = createChatMediaService(call);
        const requests = Array.from({ length: 8 }, (_, index) =>
            service.image(target, { file: `limited-${index}` }, true),
        );
        expect(finish).toHaveLength(4);
        const duplicate = service.image(target, { file: 'limited-7' }, true);
        await expect(service.record(target, { file: 'voice.silk' })).resolves.toBe(
            'data:audio/mpeg;base64,SUQzAA==',
        );
        finish.splice(0).forEach((resolve) => resolve());
        await vi.waitFor(() => expect(finish).toHaveLength(4));
        finish.splice(0).forEach((resolve) => resolve());
        await expect(Promise.all([...requests, duplicate])).resolves.toHaveLength(9);
        expect(peak).toBe(4);
        expect(call.mock.calls.filter((args) => args[1] === 'get_image')).toHaveLength(8);
    });
    it('prefers transcoded audio over SnowLuma original URLs without a format suffix', async () => {
        vi.spyOn(chatService, 'call').mockResolvedValue(
            ok({
                url: 'https://qq.example/download?id=voice',
                file: 'voice.silk',
                out_format: 'mp3',
                base64: 'SUQzAA==',
            }),
        );
        await expect(
            chatMediaService.record({ ...target, backend: 'snowluma' }, { file: 'voice' }),
        ).resolves.toBe('data:audio/mpeg;base64,SUQzAA==');
    });
    it('uses returned video bytes rather than mapping a remote path onto the desktop', async () => {
        vi.spyOn(chatService, 'call').mockResolvedValue(
            ok({ file: '/tmp/remote-video.mp4', base64: 'AAAA', file_name: 'clip.webm' }),
        );
        await expect(
            chatMediaService.video(
                { ...target, host: { kind: 'remote', server_id: 'remote' } },
                { file: 'video-id' },
            ),
        ).resolves.toBe('data:video/webm;base64,AAAA');
    });
    it('rejects empty voice transcription', async () => {
        vi.spyOn(chatService, 'call').mockResolvedValue(ok({ text: ' ' }));
        await expect(chatMediaService.transcript(target, '12345')).rejects.toThrow('没有返回内容');
    });
    it('does not present SILK URLs or local paths as playable audio', async () => {
        vi.spyOn(chatService, 'call').mockResolvedValue(
            ok({ file: '/tmp/voice.silk', url: 'https://cdn.example/voice.silk' }),
        );
        await expect(chatMediaService.record(target, { file: 'voice.silk' })).rejects.toThrow(
            '未返回可播放音频',
        );
    });
    it('uses a declared browser audio container without trusting arbitrary MIME values', async () => {
        vi.spyOn(chatService, 'call').mockResolvedValue(
            ok({ base64: 'UklGRg==', out_format: 'wav' }),
        );
        expect(await chatMediaService.record(target, { file: 'voice' })).toBe(
            'data:audio/wav;base64,UklGRg==',
        );
    });
    it('resolves opaque video identifiers through get_file', async () => {
        const call = vi.spyOn(chatService, 'call').mockResolvedValue(
            ok({
                url: 'https://cdn.example/video.mp4',
                file: 'C:\\private\\video.mp4',
                file_name: 'video.mp4',
            }),
        );
        await expect(chatMediaService.video(target, { file: 'opaque-video' })).resolves.toBe(
            'https://cdn.example/video.mp4',
        );
        expect(call).toHaveBeenCalledWith('bot', 'get_file', { file: 'opaque-video' });
    });
    it('can play a get_file base64 response without exposing a local path', async () => {
        vi.spyOn(chatService, 'call').mockResolvedValue(
            ok({ file: 'C:\\private\\video.mp4', base64: 'AAAA', file_name: 'clip.webm' }),
        );
        await expect(chatMediaService.video(target, { file: 'opaque-video' })).resolves.toBe(
            'data:video/webm;base64,AAAA',
        );
    });
    it('refreshes a failed video URL through get_file', async () => {
        const call = vi
            .spyOn(chatService, 'call')
            .mockResolvedValue(ok({ url: 'https://cdn.example/fresh.mp4' }));
        await expect(
            chatMediaService.video(
                target,
                { url: 'https://cdn.example/expired.mp4?rkey=old', file_id: 'video-id' },
                true,
            ),
        ).resolves.toBe('https://cdn.example/fresh.mp4');
        expect(call).toHaveBeenCalledWith('bot', 'get_file', { file_id: 'video-id' });
    });
    it('rejects get_file responses that contain only a local path', async () => {
        vi.spyOn(chatService, 'call').mockResolvedValue(ok({ file: 'C:\\private\\video.mp4' }));
        await expect(chatMediaService.video(target, { file: 'opaque-video' })).rejects.toThrow(
            '可播放视频地址',
        );
    });
    it('loads only valid web image URLs from favorite faces', async () => {
        vi.spyOn(chatService, 'call').mockResolvedValue(
            ok([
                'https://cdn.example/1.png',
                'javascript:alert(1)',
                '/tmp/face.png',
                'https://cdn.example/1.png',
            ]),
        );
        expect(await chatMediaService.favorites(target)).toEqual(['https://cdn.example/1.png']);
    });
    it.each(['napcat', 'snowluma'] as const)(
        'reads beyond 200 favorites until %s returns the whole list',
        async (backend) => {
            const favorites = Array.from(
                { length: 630 },
                (_, index) => `https://cdn.example/${index}.gif`,
            );
            const call = vi.fn(async (_bot, _action, params) =>
                ok(favorites.slice(0, params.count)),
            );
            expect(await createChatMediaService(call).favorites({ ...target, backend })).toEqual(
                backend === 'snowluma' ? [...favorites].reverse() : favorites,
            );
            expect(call.mock.calls.map((args) => args[2].count)).toEqual([256, 512, 1024]);
        },
    );
    it('reverses a complete SnowLuma collection once without mutating the protocol response', async () => {
        const urls = [
            'https://cdn.example/old.gif',
            'https://cdn.example/middle.gif',
            'https://cdn.example/new.gif',
        ];
        const original = [...urls];
        const call = vi.fn().mockResolvedValue(ok(urls));
        expect(
            await createChatMediaService(call).favorites({ ...target, backend: 'snowluma' }),
        ).toEqual([...urls].reverse());
        expect(urls).toEqual(original);
    });
    it('reports a failed expansion rather than silently returning a partial collection', async () => {
        const call = vi
            .fn()
            .mockResolvedValueOnce(
                ok(Array.from({ length: 256 }, (_, index) => `https://cdn.example/${index}.gif`)),
            )
            .mockRejectedValueOnce(new Error('读取失败'));
        await expect(createChatMediaService(call).favorites(target)).rejects.toThrow('读取失败');
    });
    it('uses the exact image resource before an ambiguous shared file name on refresh', async () => {
        const call = vi.fn().mockResolvedValue(ok({ url: 'https://cdn.example/correct.gif' }));
        await createChatMediaService(call).image(
            target,
            { file: '0', url: 'https://p.qpic.cn/qq_expression/99/selected/0' },
            true,
        );
        expect(call).toHaveBeenCalledWith('bot', 'get_image', {
            file: 'https://p.qpic.cn/qq_expression/99/selected/0',
        });
    });
    it('never falls back to the shared placeholder 0 when the selected sticker cannot refresh', async () => {
        const call = vi.fn().mockRejectedValue(new Error('图片已失效'));
        await expect(
            createChatMediaService(call).image(
                target,
                { file: '0', url: 'https://cdn.example/expired-sticker.gif' },
                true,
            ),
        ).rejects.toThrow('图片已失效');
        expect(call).toHaveBeenCalledOnce();
    });
    it('surfaces rejected and truncated media responses', async () => {
        const response = ok({ base64: 'SUQzAA==' });
        if (response.result.kind === 'ok') response.result.outcome.truncated = true;
        vi.spyOn(chatService, 'call').mockResolvedValue(response);
        await expect(chatMediaService.record(target, { file: 'voice' })).rejects.toThrow(
            '内容过大',
        );
    });
    it('renders a just-sent image from local bytes instead of asking the protocol', async () => {
        const call = vi.fn();
        const readLocal = vi.fn().mockResolvedValue('iVBORw0KGgo=');
        const service = createChatMediaService(call, readLocal);
        await expect(
            service.image(target, { file: 'ncd-local-file://C:\\tmp\\sent.png', name: 'sent.png' }),
        ).resolves.toBe('data:image/png;base64,iVBORw0KGgo=');
        expect(readLocal).toHaveBeenCalledWith('C:\\tmp\\sent.png');
        expect(call).not.toHaveBeenCalled();
    });
    it('prefers the carried local_file over the echoed remote address', async () => {
        const call = vi.fn();
        const readLocal = vi.fn().mockResolvedValue('/9j/4AAQ');
        const service = createChatMediaService(call, readLocal);
        await expect(
            service.image(target, {
                url: 'https://cdn.example/echo.png',
                file: 'nt-file-id',
                local_file: 'ncd-local-file://D:\\shot.jpg',
            }),
        ).resolves.toBe('data:image/jpeg;base64,/9j/4AAQ');
        expect(call).not.toHaveBeenCalled();
    });
    it('falls back to the protocol chain when the local file is gone', async () => {
        const call = vi.fn().mockResolvedValue(ok({ url: 'https://cdn.example/recovered.png' }));
        const readLocal = vi.fn().mockRejectedValue(new Error('读取本地图片失败'));
        const service = createChatMediaService(call, readLocal);
        await expect(
            service.image(
                target,
                { file: 'nt-file-id', local_file: 'ncd-local-file://D:\\gone.png' },
                true,
            ),
        ).resolves.toBe('https://cdn.example/recovered.png');
        expect(call).toHaveBeenCalledWith('bot', 'get_image', { file: 'nt-file-id' });
    });
    it('resolves base64:// local payloads without a protocol round trip', async () => {
        const call = vi.fn();
        const service = createChatMediaService(call, vi.fn());
        await expect(
            service.image(target, {
                local_file: 'base64://R0lGODlh',
                url: 'https://cdn.example/echo.gif',
            }),
        ).resolves.toBe('data:image/gif;base64,R0lGODlh');
        expect(call).not.toHaveBeenCalled();
    });
});
