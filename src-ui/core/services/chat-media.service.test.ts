import { describe, expect, it, vi } from 'vitest';
import { chatService } from './chat.service';
import { chatMediaService, createChatMediaService } from './chat-media.service';
import type { DebugCallResponse } from '../ipc/generated/debug/DebugCallResponse';
import type { DebugTarget } from '../ipc/generated/debug/DebugTarget';
vi.mock('../ipc/transport', async original => ({ ...await original<typeof import('../ipc/transport')>(), isTauri: true }));
vi.mock('@tauri-apps/api/core', async original => ({ ...await original<typeof import('@tauri-apps/api/core')>(), convertFileSrc: (path: string) => `asset://localhost/${path}` }));
const target: DebugTarget = { bot_id: 'bot', name: '测试', qq_id: 99, backend: 'napcat', host: { kind: 'local' }, running: true, online: true };
const ok = (data: unknown): DebugCallResponse => ({ request_id: 'req', result: { kind: 'ok', outcome: { ok: true, data, status: 'ok', retcode: 0, message: '', wording: '', raw: {}, channel: { kind: 'internal' }, elapsed_ms: 1, size_bytes: 0, truncated: false } } });
describe('native chat media protocol', () => {
    it('uses an injected debug caller without opening a native chat transport', async () => {
        const native = vi.spyOn(chatService, 'call');
        const debug = vi.fn().mockResolvedValue(ok({ url: 'https://cdn.example/debug.png' }));
        await expect(createChatMediaService(debug).image(target, { file: 'image-id' })).resolves.toBe('https://cdn.example/debug.png');
        expect(debug).toHaveBeenCalledWith('bot', 'get_image', { file: 'image-id' });
        expect(native).not.toHaveBeenCalled();
    });
    it.each(['napcat', 'snowluma'] as const)('reads %s forwards using its resource identifier parameter', async backend => {
        const call = vi.spyOn(chatService, 'call').mockResolvedValue(ok({ messages: [{ sender: { user_id: 123, nickname: '小明' }, time: 1700000000, message: [{ type: 'text', data: { text: '内容' } }, { type: 'forward', data: { id: 'nested' } }] }] }));
        const nodes = await chatMediaService.forward({ ...target, backend }, { id: 'resource-id' });
        expect(call).toHaveBeenCalledWith('bot', 'get_forward_msg', backend === 'napcat' ? { message_id: 'resource-id' } : { id: 'resource-id' });
        expect(nodes).toEqual([{ senderId: '123', name: '小明', time: 1700000000000, segments: [{ type: 'text', data: { text: '内容' } }, { type: 'forward', data: { id: 'nested' } }] }]);
    });
    it('normalizes inline node content without another protocol request', async () => {
        const call = vi.spyOn(chatService, 'call');
        const nodes = await chatMediaService.forward(target, { content: [{ type: 'node', data: { user_id: '123', nickname: '小明', content: '[CQ:face,id=14]你好' } }] });
        expect(nodes[0]).toMatchObject({ senderId: '123', name: '小明', segments: [{ type: 'face', data: { id: '14' } }, { type: 'text', data: { text: '你好' } }] });
        expect(call).not.toHaveBeenCalled();
    });
    it('requests playable audio and ignores an upstream local output path', async () => {
        const call = vi.spyOn(chatService, 'call').mockResolvedValue(ok({ file: 'C:\private\voice.mp3', url: 'C:\private\voice.mp3', base64: 'SUQzAA==' }));
        expect(await chatMediaService.record(target, { file: 'voice.silk', url: 'https://cdn.example/voice' })).toBe('data:audio/mpeg;base64,SUQzAA==');
        expect(call).toHaveBeenCalledWith('bot', 'get_record', { file: 'voice.silk', out_format: 'mp3' });
    });
    it('requests QQ voice transcription by message id', async () => {
        const call = vi.spyOn(chatService, 'call').mockResolvedValue(ok({ text: '  你好，世界  ' }));
        await expect(chatMediaService.transcript(target, '12345')).resolves.toBe('你好，世界');
        expect(call).toHaveBeenCalledWith('bot', 'fetch_ptt_text', { message_id: '12345' });
    });
    it('refreshes an image URL through get_image instead of reusing an expired rkey', async () => {
        const call = vi.spyOn(chatService, 'call').mockResolvedValue(ok({ url: 'https://cdn.example/image.png?rkey=fresh' }));
        await expect(chatMediaService.image(target, { file: 'https://cdn.example/image.png?rkey=expired' }, true)).resolves.toBe('https://cdn.example/image.png?rkey=fresh');
        expect(call).toHaveBeenCalledWith('bot', 'get_image', { file: 'https://cdn.example/image.png?rkey=expired' });
    });
    it('accepts base64:// audio returned by get_record', async () => {
        vi.spyOn(chatService, 'call').mockResolvedValue(ok({ file: 'base64://UklGRg==', out_format: 'wav' }));
        await expect(chatMediaService.record(target, { file: 'voice.silk' })).resolves.toBe('data:audio/wav;base64,UklGRg==');
    });
    it('prefers transcoded audio over SnowLuma original URLs without a format suffix', async () => {
        vi.spyOn(chatService, 'call').mockResolvedValue(ok({ url: 'https://qq.example/download?id=voice', file: 'voice.silk', out_format: 'mp3', base64: 'SUQzAA==' }));
        await expect(chatMediaService.record({ ...target, backend: 'snowluma' }, { file: 'voice' })).resolves.toBe('data:audio/mpeg;base64,SUQzAA==');
    });
    it('uses returned video bytes rather than mapping a remote path onto the desktop', async () => {
        vi.spyOn(chatService, 'call').mockResolvedValue(ok({ file: '/tmp/remote-video.mp4', base64: 'AAAA', file_name: 'clip.webm' }));
        await expect(chatMediaService.video({ ...target, host: { kind: 'remote', server_id: 'remote' } }, { file: 'video-id' })).resolves.toBe('data:video/webm;base64,AAAA');
    });
    it('rejects empty voice transcription', async () => {
        vi.spyOn(chatService, 'call').mockResolvedValue(ok({ text: ' ' }));
        await expect(chatMediaService.transcript(target, '12345')).rejects.toThrow('没有返回内容');
    });
    it('does not present SILK URLs or local paths as playable audio', async () => {
        vi.spyOn(chatService, 'call').mockResolvedValue(ok({ file: '/tmp/voice.silk', url: 'https://cdn.example/voice.silk' }));
        await expect(chatMediaService.record(target, { file: 'voice.silk' })).rejects.toThrow('未返回可播放音频');
    });
    it('uses a declared browser audio container without trusting arbitrary MIME values', async () => {
        vi.spyOn(chatService, 'call').mockResolvedValue(ok({ base64: 'UklGRg==', out_format: 'wav' }));
        expect(await chatMediaService.record(target, { file: 'voice' })).toBe('data:audio/wav;base64,UklGRg==');
    });
    it('resolves opaque video identifiers through get_file', async () => {
        const call = vi.spyOn(chatService, 'call').mockResolvedValue(ok({ url: 'https://cdn.example/video.mp4', file: 'C:\\private\\video.mp4', file_name: 'video.mp4' }));
        await expect(chatMediaService.video(target, { file: 'opaque-video' })).resolves.toBe('https://cdn.example/video.mp4');
        expect(call).toHaveBeenCalledWith('bot', 'get_file', { file: 'opaque-video' });
    });
    it('can play a get_file base64 response without exposing a local path', async () => {
        vi.spyOn(chatService, 'call').mockResolvedValue(ok({ file: 'C:\\private\\video.mp4', base64: 'AAAA', file_name: 'clip.webm' }));
        await expect(chatMediaService.video(target, { file: 'opaque-video' })).resolves.toBe('data:video/webm;base64,AAAA');
    });
    it('refreshes a failed video URL through get_file', async () => {
        const call = vi.spyOn(chatService, 'call').mockResolvedValue(ok({ url: 'https://cdn.example/fresh.mp4' }));
        await expect(chatMediaService.video(target, { url: 'https://cdn.example/expired.mp4?rkey=old', file_id: 'video-id' }, true)).resolves.toBe('https://cdn.example/fresh.mp4');
        expect(call).toHaveBeenCalledWith('bot', 'get_file', { file_id: 'video-id' });
    });
    it('rejects get_file responses that contain only a local path', async () => {
        vi.spyOn(chatService, 'call').mockResolvedValue(ok({ file: 'C:\\private\\video.mp4' }));
        await expect(chatMediaService.video(target, { file: 'opaque-video' })).rejects.toThrow('可播放视频地址');
    });
    it('loads only valid web image URLs from favorite faces', async () => {
        vi.spyOn(chatService, 'call').mockResolvedValue(ok(['https://cdn.example/1.png', 'javascript:alert(1)', '/tmp/face.png', 'https://cdn.example/1.png']));
        expect(await chatMediaService.favorites(target)).toEqual(['https://cdn.example/1.png']);
    });
    it('surfaces rejected and truncated media responses', async () => {
        const response = ok({ base64: 'SUQzAA==' });
        if (response.result.kind === 'ok') response.result.outcome.truncated = true;
        vi.spyOn(chatService, 'call').mockResolvedValue(response);
        await expect(chatMediaService.record(target, { file: 'voice' })).rejects.toThrow('内容过大');
    });
});
