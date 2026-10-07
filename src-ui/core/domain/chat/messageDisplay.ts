// 只折叠可证明的媒体占位副本；消息原文和发送数据保持原样。
import type { Segment } from '../debug/segments';
import { projectQQFaceDisplay, type QQFaceDisplaySegment } from './qqFaces';

function marketFace(segment: Segment): boolean {
    if (segment.type === 'mface') return true;
    if (segment.type !== 'image') return false;
    const data = segment.data;
    const packageId = data.emoji_package_id ?? data.package_id ?? data.tab_id;
    const emojiId = typeof data.emoji_id === 'number' ? String(data.emoji_id) : data.emoji_id;
    return (
        typeof emojiId === 'string' &&
        !!emojiId.trim() &&
        (typeof packageId === 'number' ||
            (typeof packageId === 'string' && /^\d+$/.test(packageId))) &&
        Number.isSafeInteger(Number(packageId)) &&
        Number(packageId) >= 0
    );
}
export function projectMessageDisplay(
    segments: readonly Segment[],
    lookup?: Parameters<typeof projectQQFaceDisplay>[1],
): QQFaceDisplaySegment[] {
    const faces = projectQQFaceDisplay(segments, lookup);
    const displayed: QQFaceDisplaySegment[] = [];
    for (let index = 0; index < faces.length; index++) {
        const segment = faces[index];
        displayed.push(segment);
        if (!marketFace(segment)) continue;
        const next = faces[index + 1];
        if (next?.type !== 'text' || typeof next.data.text !== 'string') continue;
        const labels = [segment.data.summary, segment.data.name].flatMap((value) => {
            if (typeof value !== 'string' || !value.trim()) return [];
            const name = value.trim();
            return [name.startsWith('[') && name.endsWith(']') ? name : '[' + name + ']'];
        });
        if (labels.includes(next.data.text.trim())) index++;
    }
    return displayed;
}
