// 只消费协议返回的媒体内容，不把主机文件路径交给 WebView。
import { chatService } from './chat.service';
import { id, record, text } from '../domain/chat/model';
import { normalizeMessage, type Segment } from '../domain/debug/segments';
import { isLocalFileToken, LOCAL_FILE_PREFIX } from '../domain/debug/streamActions';
import { debugErrorCopy } from '../domain/debug/errorCopy';
import type { DebugCallResponse } from '../ipc/generated/debug/DebugCallResponse';
import type { DebugTarget } from '../ipc/generated/debug/DebugTarget';

export interface ForwardNode { senderId: string; name: string; time?: number; segments: Segment[] }
function dataOf(response: DebugCallResponse): unknown {
    if (response.result.kind === 'err') { const copy = debugErrorCopy(response.result.error); throw new Error([copy.title, copy.detail].filter(Boolean).join('：')); }
    const result = response.result.outcome;
    if (!result.ok) throw new Error(result.wording || result.message || `请求失败（${result.retcode}）`);
    if (result.truncated) throw new Error('媒体内容过大，无法完整读取');
    return result.data;
}
function playableUrl(value: unknown): string {
    const candidate = text(value);
    return /^(https?:\/\/|data:(?:video|audio|image)\/|blob:)/i.test(candidate) ? candidate : '';
}
function base64Payload(result: Record<string, unknown>): string {
    for (const [key, value] of Object.entries(result)) {
        const raw = text(value).replace(/\s/g, '');
        if (!raw) continue;
        if (key === 'base64') return raw.replace(/^base64:\/\//i, '');
        if (/^base64:\/\//i.test(raw)) return raw.slice('base64://'.length);
    }
    return '';
}
function imageBytes(result: Record<string, unknown>): string {
    const value = base64Payload(result);
    if (!value) return '';
    if (value.length > 16 * 1024 * 1024 || value.length % 4 !== 0 || !/^[A-Za-z0-9+/]+={0,2}$/.test(value)) throw new Error('图片内容不完整或过大');
    const mime = value.startsWith('/9j/') ? 'image/jpeg' : value.startsWith('R0lGOD') ? 'image/gif' : value.startsWith('UklGR') ? 'image/webp' : 'image/png';
    return `data:${mime};base64,${value}`;
}
function videoMime(name: string): string {
    const extension = name.toLowerCase().match(/\.([a-z0-9]+)(?:$|[?#])/i)?.[1];
    return extension === 'webm' ? 'video/webm' : extension === 'mov' ? 'video/quicktime' : extension === 'ogg' || extension === 'ogv' ? 'video/ogg' : extension === 'm3u8' ? 'application/vnd.apple.mpegurl' : 'video/mp4';
}
function forwardNodes(value: unknown): ForwardNode[] {
    if (!Array.isArray(value)) throw new Error('此协议未返回可识别的聊天记录');
    if (value.length > 500) throw new Error('转发消息过多，暂时无法展开');
    return value.map(raw => {
        const row = record(raw); const data = row.type === 'node' ? record(row.data) : row; const sender = record(data.sender);
        const senderId = id(sender.user_id) || id(data.user_id) || id(data.uin);
        const content = data.message ?? data.content ?? data.raw_message;
        // NapCat 内嵌转发有时返回 node 段，保持为可按需展开的安全消息段。
        const segments = normalizeMessage(content).map(segment => segment.type === 'node' ? { type: 'forward', data: { content: [segment] } } : segment);
        return { senderId, name: text(sender.card) || text(sender.nickname) || text(data.nickname) || text(data.name) || senderId || '未知发送者', time: typeof data.time === 'number' ? data.time * 1000 : undefined, segments };
    });
}
export function createChatMediaService(call: typeof chatService.call = (...args) => chatService.call(...args), readLocal: (path: string) => Promise<string> = path => chatService.readLocalImage(path)) {
    const pendingImages = new Map<string, Promise<string>>();
    // 自己刚发的图以本机字节为准；同步判源，无本地来源时不改变原有的调用时序。
    const localToken = (data: Record<string, unknown>): string => [text(data.local_file), text(data.file)].find(value => value.startsWith('base64://') || isLocalFileToken(value)) ?? '';
    const resolveImage = async (target: DebugTarget, data: Record<string, unknown>, refresh: boolean): Promise<string> => {
        // 文件名可能在不同消息中复用；完整资源地址能避免 get_image 命中另一张同名图。
        const identifiers = [...new Set([text(data.url), text(data.file_id), text(data.file)].filter(value => value && value !== '0'))];
        if (!identifiers.length) throw new Error('这张图片缺少文件标识');
        let failure: unknown = new Error('协议未返回可播放图片地址');
        for (const file of identifiers) {
            try {
                const result = record(dataOf(await call(target.bot_id, 'get_image', { file })));
                const bytes = imageBytes(result);
                if (bytes) return bytes;
                const url = playableUrl(result.url) || playableUrl(result.file);
                if (url) return url;
            } catch (error) { failure = error; }
        }
        // 刷新不能回退到已知失效的签名地址。
        if (!refresh) { const direct = playableUrl(data.url) || playableUrl(data.file); if (direct) return direct; }
        throw failure;
    };
    return {
    async image(target: DebugTarget, data: Record<string, unknown>, refresh = false): Promise<string> {
        const token = localToken(data);
        if (token) {
            try {
                const raw = token.startsWith('base64://') ? token : await readLocal(token.slice(LOCAL_FILE_PREFIX.length));
                const bytes = imageBytes({ base64: raw });
                if (bytes) return bytes;
            } catch { /* 本机文件可能已被移动，回退协议链路 */ }
        }
        const direct = playableUrl(data.url) || playableUrl(data.file) || imageBytes({ file: data.file });
        if (direct && !refresh) return direct;
        const key = JSON.stringify([target.bot_id, target.qq_id, refresh, data.file_id, data.file, data.url]);
        const existing = pendingImages.get(key); if (existing) return existing;
        const request = resolveImage(target, data, refresh);
        if (pendingImages.size < 256) pendingImages.set(key, request);
        try { return await request; } finally { if (pendingImages.get(key) === request) pendingImages.delete(key); }
    },
    async forward(target: DebugTarget, data: Record<string, unknown>): Promise<ForwardNode[]> {
        const inline = data.content ?? data.messages;
        if (Array.isArray(inline) && inline.length) return forwardNodes(inline);
        const resourceId = id(data.id) || id(data.res_id) || id(data.forward_id) || id(data.message_id);
        if (!resourceId) { if (Array.isArray(inline)) return []; throw new Error('这条聊天记录缺少可读取的标识'); }
        const params = target.backend === 'snowluma' ? { id: resourceId } : { message_id: resourceId };
        return forwardNodes(record(dataOf(await call(target.bot_id, 'get_forward_msg', params))).messages);
    },
    async record(target: DebugTarget, data: Record<string, unknown>): Promise<string> {
        const file = text(data.file) || text(data.file_id) || text(data.url);
        if (!file) throw new Error('这条语音缺少文件标识');
        const result = record(dataOf(await call(target.bot_id, 'get_record', { file, out_format: 'mp3' })));
        const base64 = base64Payload(result);
        // SnowLuma 转码后仍带原 SILK 地址，必须先消费转码字节。
        if (base64) {
            if (base64.length % 4 !== 0 || !/^[A-Za-z0-9+/]+={0,2}$/.test(base64)) throw new Error('协议返回的音频内容不完整');
            if (base64.length > 16 * 1024 * 1024) throw new Error('媒体内容过大，无法播放');
            const mime = result.out_format === 'wav' ? 'audio/wav' : result.out_format === 'ogg' ? 'audio/ogg' : 'audio/mpeg';
            return 'data:' + mime + ';base64,' + base64;
        }
        const url = playableUrl(result.url) || playableUrl(result.file);
        if (url && !/\.silk(?:$|[?#])/i.test(url)) return url;
        throw new Error('协议未返回可播放音频，请确认服务端支持 MP3 转码后重试');
    },
    async video(target: DebugTarget, data: Record<string, unknown>, refresh = false): Promise<string> {
        const direct = playableUrl(data.url) || playableUrl(data.file) || playableUrl(data.path);
        if (direct && !refresh) return direct;
        const file = text(data.file_id) || text(data.file) || text(data.path);
        if (!file) throw new Error('这条视频缺少文件标识');
        const params = text(data.file_id) ? { file_id: file } : { file };
        const result = record(dataOf(await call(target.bot_id, 'get_file', params)));
        const url = playableUrl(result.url);
        if (url) return url;
        const base64 = base64Payload(result);
        if (base64 && base64.length % 4 === 0 && /^[A-Za-z0-9+/]+={0,2}$/.test(base64)) {
            if (base64.length > 16 * 1024 * 1024) throw new Error('视频内容过大，无法播放');
            return `data:${videoMime(text(result.file_name) || file)};base64,${base64}`;
        }
        throw new Error('协议未返回可播放视频地址');
    },
    async transcript(target: DebugTarget, messageId: string): Promise<string> {
        if (!messageId.trim()) throw new Error('这条语音缺少消息标识');
        const result = record(dataOf(await call(target.bot_id, 'fetch_ptt_text', { message_id: messageId })));
        const value = text(result.text).trim();
        if (!value) throw new Error('语音转文字没有返回内容');
        return value;
    },
    async favorites(target: DebugTarget): Promise<string[]> {
        // 两端接口均为前 N 项而非游标分页；扩大窗口直到返回不足一页。
        for (let count = 256; count <= 65_536; count *= 2) {
            const result = dataOf(await call(target.bot_id, 'fetch_custom_face', { count }));
            if (!Array.isArray(result)) throw new Error('此协议未返回收藏表情列表');
            if (result.length < count) {
                const urls = [...new Set(result.filter((url): url is string => typeof url === 'string' && /^https?:\/\//i.test(url)))];
                // Faceroam 的存储顺序与 QQ 面板相反；NapCat 已返回面板顺序。
                return target.backend === 'snowluma' ? urls.reverse() : urls;
            }
        }
        throw new Error('收藏表情过多，无法完整读取');
    },
    };
}
export const chatMediaService = createChatMediaService();
