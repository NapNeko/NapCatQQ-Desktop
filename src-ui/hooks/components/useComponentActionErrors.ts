// 监听 componentActionStore，把 failed / cancelled 终态 task 推进全局 InfoBar 队列。
//
// 这是一个"副作用"hook，不返回值。挂载位置：ComponentsPage 顶层（拿得到
// rows 反查显示名）。banner 真正渲染由 AppNext 顶层的单例 InfoBarStack 完成。
//
// 行为约定：
//   - 仅 failed / cancelled 推 banner。success 已经在 row 里有 ✓ 反馈，
//     不需要顶部再 toast 一遍。
//   - 一个 task 只推一次（用 seenIds 去重）。用户 dismiss 之后即使 store
//     里 task 仍在，也不会再次弹出。
//   - banner 持有完整错误文本（title + content），不截断；点 close 才消失。
//   - banner 的 dismiss 由全局 store 处理（InfoBarStack 调
//     globalInfoBarStore.dismiss）；本 hook 只负责"什么时候 push"。

import { useEffect, useRef, useSyncExternalStore } from 'react';
import { componentActionStore } from './componentActionStore';
import type { ActionProgressView } from '../../core/domain/components/progress';
import type { ComponentRow } from '../../core/domain/components/types';
import type { ComponentId } from '../../core/ipc/types';
import { globalInfoBarStore } from '../ui/globalInfoBarStore';

/// 从 progress.logs 倒序找最近一条 error / warn 记录的 message。
/// 没找到回退到 progress.message。
function pickErrorMessage(progress: ActionProgressView): string {
    for (let i = progress.logs.length - 1; i >= 0; i--) {
        const log = progress.logs[i];
        if (log.level === 'error' || log.level === 'warn') return log.message;
    }
    return progress.message || '未知错误';
}

/// 在 ComponentRow 列表里反查 (componentId, hostId) → "NapCat · 本机" 标题。
/// 找不到时退化成裸 id。
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

/**
 * 副作用 hook：组件页顶层调一次，把 component-action 失败 / 取消转 banner。
 *
 * 用法：
 * ```ts
 * useComponentActionErrors(allRows);  // 副作用：终态自动推 banner
 * ```
 *
 * 不返回值：banner 显示统一靠 AppNext 顶层的 InfoBarStack。
 */
export function useComponentActionErrors(rows: ComponentRow[]): void {
    const state = useSyncExternalStore(
        componentActionStore.subscribe,
        componentActionStore.getSnapshot,
        componentActionStore.getSnapshot,
    );

    // 已经推过 banner 的 task_id 集合，避免每次 store 变化重复推。
    // 用 ref 不触发 re-render。
    const seenIdsRef = useRef<Set<string>>(new Set());

    // rows 变化时 display 结果可能更新；用 ref 记最新版本。banner 在 push
    // 那一刻拍下当时的显示名快照（store 是冻结的字符串，rows 后续变化不会
    // 倒回去刷新已推 banner）。
    const rowsRef = useRef(rows);
    rowsRef.current = rows;

    useEffect(() => {
        // 每次 store 变化，扫一遍最近进入终态的 failed / cancelled task，
        // 没在 seen 集合里的就推一条新 banner。
        for (const [taskId, progress] of Object.entries(state.tasks)) {
            const status = progress.status;
            if (status !== 'failed' && status !== 'cancelled') continue;
            if (seenIdsRef.current.has(taskId)) continue;
            seenIdsRef.current.add(taskId);

            const target = state.taskTargets[taskId];
            const heading = target
                ? resolveDisplay(rowsRef.current, target.componentId, target.hostId)
                : '组件操作';
            const isCancelled = status === 'cancelled';
            // 走 store.push 直接调用而非 hook，避免在 useEffect 里依赖
            // useGlobalInfoBars().push 引用稳定性。
            globalInfoBarStore.push({
                // task_id 当 banner id 直接传进 push 走"同 key 顶替"，
                // 防止开发模式 strict mode 重 effect 也只推一条。
                key: `component-action:${taskId}`,
                tone: isCancelled ? 'warning' : 'danger',
                title: `${heading} · ${isCancelled ? '已取消' : '失败'}`,
                content: pickErrorMessage(progress),
                // 错误条不自动消失，让用户读完再关。取消条 8 秒淡出（属于
                // 用户主动行为，不需要长期占屏）。
                autoDismissMs: isCancelled ? 8000 : 0,
            });
        }
    }, [state]);
}
