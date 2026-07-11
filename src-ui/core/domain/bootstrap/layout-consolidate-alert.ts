// 布局收敛 InfoBar 的纯展示逻辑（可单测，不依赖 React）。

import type { DataLayoutConsolidateSnapshot } from '../../ipc/types';

export type LayoutConsolidateAlert =
    | { kind: 'none' }
    | {
        kind: 'success' | 'warning';
        title: string;
        content: string;
        autoDismissMs: number;
    };

export function buildLayoutConsolidateContent(
    snap: DataLayoutConsolidateSnapshot,
): string {
    const parts: string[] = [];
    if (snap.backup_path) {
        parts.push(`备份：${snap.backup_path}（含密钥，请自行保管）`);
    }
    if (snap.moved_count > 0) {
        parts.push(`整理 ${snap.moved_count} 项`);
    }
    if (snap.warnings.length > 0) {
        parts.push(snap.warnings.slice(0, 2).join('；'));
    }
    if (snap.error) {
        parts.push(snap.error);
        if (!snap.backup_path) {
            parts.push('原数据目录未删除，可查看日志后重试。');
        }
    }
    return parts.join('。') || '数据目录已检查。';
}

/** 根据 layout_consolidate 决定是否弹 InfoBar 及文案。 */
export function resolveLayoutConsolidateAlert(
    layout: DataLayoutConsolidateSnapshot | null | undefined,
): LayoutConsolidateAlert {
    if (!layout) return { kind: 'none' };
    // 仅 GC / 已是目标布局：不打扰
    if (!layout.performed && !layout.error) return { kind: 'none' };

    if (layout.error) {
        return {
            kind: 'warning',
            title: '数据目录整理未完成',
            content: buildLayoutConsolidateContent(layout),
            autoDismissMs: 0,
        };
    }

    return {
        kind: layout.warnings.length > 0 ? 'warning' : 'success',
        title: '数据目录已整理',
        content: buildLayoutConsolidateContent(layout),
        autoDismissMs: 10_000,
    };
}
