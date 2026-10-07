// 转发路径保存消息锚点及有界行高，避免返回时按新估算跳到另一条消息。
import type { VirtualItem } from '@tanstack/react-virtual';
export interface ForwardPosition {
    scroll: number;
    anchor?: { index: number; offset: number };
    measurements: VirtualItem[];
}
export function captureForwardPosition(
    scroll: number,
    measurements: VirtualItem[],
): ForwardPosition {
    const rows = measurements.slice(0, 501).map((row) => ({ ...row }));
    const first = rows.find((row) => row.end > scroll) ?? rows[rows.length - 1];
    return {
        scroll,
        measurements: rows,
        ...(first
            ? { anchor: { index: first.index, offset: Math.max(0, scroll - first.start) } }
            : {}),
    };
}
export function forwardPositionOffset(position: ForwardPosition, count: number): number {
    if (!position.anchor) return position.scroll;
    const index = Math.max(0, Math.min(position.anchor.index, count - 1));
    const row = position.measurements.find((item) => item.index === index);
    const start = row?.start ?? (index === 0 ? 0 : 48 + (index - 1) * 110);
    return start + position.anchor.offset;
}
