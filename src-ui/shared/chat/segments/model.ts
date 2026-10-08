// 段渲染的共用底座：取值 / 类型探测 / 媒体地址提取 / 渲染上限 / 图片尺寸常量。
// 这里不允许出现 React 依赖，各段组件各自引用自己需要的部分。

import type { Segment } from '../../../core/domain/debug/segments';
import { projectMessageDisplay } from '../../../core/domain/chat/messageDisplay';
import { qqFaceLarge, type QQSystemFace } from '../../../core/domain/chat/qqFaces';

export const str = (v: unknown): string =>
    typeof v === 'string' ? v : typeof v === 'number' ? String(v) : '';
export const isRecord = (v: unknown): v is Record<string, unknown> =>
    typeof v === 'object' && v !== null && !Array.isArray(v);

/** 只有图（或表情包）的消息不画气泡底，和 QQ 一样 */
export function isPictureOnly(segments: readonly Segment[]): boolean {
    return (
        segments.length > 0 &&
        projectMessageDisplay(segments).every((s) => s.type === 'image' || s.type === 'mface')
    );
}

/** 图片、表情、语音、视频和合并转发单独成消息时，让内容直接贴在时间线上。 */
export function isMediaOnly(
    segments: readonly Segment[],
    peekFace: (id: string) => QQSystemFace | undefined,
): boolean {
    return (
        segments.length > 0 &&
        projectMessageDisplay(segments, peekFace).every(
            (s) =>
                ['image', 'mface', 'record', 'video', 'forward'].includes(s.type) ||
                (s.type === 'face' && (qqFaceLarge(s.data) ?? s.displayLarge)),
        )
    );
}

/** 一段文字最多画这么多字；再长的去详情里看，免得一条消息卡住整栏 */
export const TEXT_RENDER_CAP = 20_000;

// 网址后面紧跟的中文标点、右括号不算网址的一部分
export const URL_RE = /https?:\/\/[^\s<>"'`，。！？、；：（）【】《》「」)\]}]+/g;

/** 能直接给 <img> 用的地址：url 优先，file 是网址或 base64 时也认 */
export function imageUrlOf(d: Record<string, unknown>): string {
    for (const v of [d.url, d.file]) {
        const s = str(v);
        if (/^(https?:|data:image\/)/i.test(s)) return s;
        if (s.startsWith('base64://'))
            return `data:image/png;base64,${s.slice('base64://'.length)}`;
    }
    return '';
}

export function mediaUrlOf(d: Record<string, unknown>): string {
    for (const v of [d.url, d.file, d.path]) {
        const s = str(v);
        if (s) return s;
    }
    return '';
}

export const IMAGE_BOX_H = 280;
export const IMAGE_MAX_W = 320;
export const IMAGE_PLACEHOLDER_W = IMAGE_MAX_W;
export const STICKER_BOX = 128;
