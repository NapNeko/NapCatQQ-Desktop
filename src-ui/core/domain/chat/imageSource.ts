import type { Segment } from '../debug/segments';
import { isLocalFileToken } from '../debug/streamActions';

export const INLINE_IMAGE_PREFIX = 'ncd-inline-image://';
export const isInlineImageReference = (
    value: unknown,
): value is `${typeof INLINE_IMAGE_PREFIX}${string}` =>
    typeof value === 'string' && value.startsWith(INLINE_IMAGE_PREFIX);

const imageSegment = (segment: Segment) => segment.type === 'image' || segment.type === 'mface';
const localSource = (data: Record<string, unknown>) =>
    [data.inline_ref, data.local_file, data.file].find(
        (value): value is string =>
            typeof value === 'string' &&
            (/^(base64:\/\/|data:image\/)/i.test(value) ||
                isInlineImageReference(value) ||
                isLocalFileToken(value)),
    );
const recoverableContent = (value: unknown, depth = 0): boolean => {
    if (!value || typeof value !== 'object' || depth >= 16) return false;
    if (Array.isArray(value)) return value.some((item) => recoverableContent(item, depth + 1));
    const data = value as Record<string, unknown>;
    return (
        !!localSource(data) ||
        typeof data.source_message_id === 'string' ||
        Object.values(data).some((item) => recoverableContent(item, depth + 1))
    );
};

// 按完整图片序列对齐，不能先过滤本地来源，否则混合图片会错配。
export function withLocalImageSources(segments: Segment[], previous: Segment[]): Segment[] {
    const images = previous.filter(imageSegment);
    const aligned = images.length === segments.filter(imageSegment).length;
    const forwards = previous.filter((segment) => segment.type === 'forward');
    const forwardsAligned =
        forwards.length === segments.filter((segment) => segment.type === 'forward').length;
    let imageIndex = 0;
    let forwardIndex = 0;
    let changed = false;
    const result = segments.map((segment) => {
        if (segment.type === 'forward') {
            const prior = forwards[forwardIndex++];
            const content = prior?.data.content ?? prior?.data.messages;
            if (
                forwardsAligned &&
                Array.isArray(content) &&
                !Array.isArray(segment.data.content) &&
                !Array.isArray(segment.data.messages) &&
                recoverableContent(content)
            ) {
                changed = true;
                return { ...segment, data: { ...segment.data, content } };
            }
            return segment;
        }
        if (!imageSegment(segment)) return segment;
        const prior = images[imageIndex++];
        if (!aligned || localSource(segment.data)) return segment;
        const local = prior && localSource(prior.data);
        if (!local) return segment;
        changed = true;
        return { ...segment, data: { ...segment.data, local_file: local } };
    });
    return changed ? result : segments;
}
