// 把 componentActionStore 里"刚进入终态"的 task 转成 InfoBar banner 列表。
//
// 行为：
//   - 只对 failed / cancelled 发 banner。success 已经在 row 里有 ✓ 反馈，
//     不需要顶部再 toast 一遍。
//   - 一个 task 只发一条 banner（用 seenIds 去重）。用户 dismiss 后从列表里
//     移除；但 store 里 task 仍在，不会被再次推上来。
//   - banner 持有完整错误文本（title + content），不截断；点开关才消失。
//
// 用法：组件页顶层 const { banners, dismiss } = useComponentActionErrors(rows);
//      <InfoBarStack items={banners} onDismiss={dismiss} />

import { useCallback, useEffect, useRef, useState, useSyncExternalStore } from 'react';
import { componentActionStore } from './componentActionStore';
import type { ActionProgressView } from '../../core/domain/components/progress';
import type { ComponentRow } from '../../core/domain/components/types';
import type { InfoBarStackItem } from '../../shared/ui';
import type { ComponentId } from '../../core/ipc/types';

interface ErrorBanner extends InfoBarStackItem {
    /** taskId 当 banner 唯一 id，方便 dismiss 反查。 */
    id: string;
}

/**
 * 从 progress.logs 倒序找最近一条 error / warn 记录的 message。
 * 没找到回退到 progress.message。
 */
function pickErrorMessage(progress: ActionProgressView): string {
    for (let i = progress.logs.length - 1; i >= 0; i--) {
        const log = progress.logs[i];
        if (log.level === 'error' || log.level === 'warn') return log.message;
    }
    return progress.message || '未知错误';
}

/**
 * 在 ComponentRow 列表里反查 (componentId, hostId) → "NapCat · 本机" 标题。
 * 找不到时退化成裸 id。
 */
function resolveDisplay(
    rows: ComponentRow[],
    componentId: ComponentId,
    hostId: string,
): string {
    const row = rows.find((r) => r.info.id === componentId);
    if (!row) return `${componentId} · ${hostId}`;
    const hostRow = row.rows.find((h) => h.host.host_id === hostId);
    const hostName = hostRow?.host.display_name ?? hostId;
    return `${row.info.display_name} · ${hostName}`;
}

export interface UseComponentActionErrorsResult {
    banners: ErrorBanner[];
    dismiss: (id: string) => void;
}

export function useComponentActionErrors(
    rows: ComponentRow[],
): UseComponentActionErrorsResult {
    const state = useSyncExternalStore(
        componentActionStore.subscribe,
        componentActionStore.getSnapshot,
        componentActionStore.getSnapshot,
    );

    // 已经显示过 / 已被关掉的 task 集合，去重 + 防重复弹。
    // 用 ref 避免 set state 触发新 subscribe 循环。
    const seenIdsRef = useRef<Set<string>>(new Set());
    const dismissedIdsRef = useRef<Set<string>>(new Set());

    const [banners, setBanners] = useState<ErrorBanner[]>([]);

    // rows 变化时 display 结果可能更新；用 ref 记最新版本，banner 生成时拿。
    const rowsRef = useRef(rows);
    rowsRef.current = rows;

    useEffect(() => {
        // 每次 store 变化，扫一遍最近进入终态的 failed / cancelled task，
        // 没在 seen / dismissed 集合里的就推一条新 banner。
        const newBanners: ErrorBanner[] = [];
        for (const [taskId, progress] of Object.entries(state.tasks)) {
            const status = progress.status;
            if (status !== 'failed' && status !== 'cancelled') continue;
            if (seenIdsRef.current.has(taskId)) continue;
            if (dismissedIdsRef.current.has(taskId)) continue;
            seenIdsRef.current.add(taskId);

            const target = state.taskTargets[taskId];
            const heading = target
                ? resolveDisplay(rowsRef.current, target.componentId, target.hostId)
                : '组件操作';
            const isCancelled = status === 'cancelled';
            newBanners.push({
                id: taskId,
                tone: isCancelled ? 'warning' : 'danger',
                title: `${heading} · ${isCancelled ? '已取消' : '失败'}`,
                content: pickErrorMessage(progress),
                // 错误条不自动消失，让用户读完再关。取消条 8 秒淡出（属于
                // 用户主动行为，不需要长期占屏）。
                autoDismissMs: isCancelled ? 8000 : 0,
            });
        }
        if (newBanners.length > 0) {
            setBanners((prev) => [...prev, ...newBanners]);
        }
    }, [state]);

    const dismiss = useCallback((id: string) => {
        dismissedIdsRef.current.add(id);
        setBanners((prev) => prev.filter((b) => b.id !== id));
    }, []);

    return { banners, dismiss };
}
