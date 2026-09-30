// 接口安全分级在界面上的样子：色点、徽章色调、短名、一句解释。
// 左栏目录和收藏、中栏请求头和接口名建议、命令面板都从这一份拿，三栏的颜色和说法才对得上。

import type { DebugActionSafety } from '../../ipc/generated/debug/DebugActionSafety';
import type { BackendType } from '../../ipc/generated/domain/BackendType';
import type { DebugActionSummary } from '../../ipc/generated/debug/DebugActionSummary';

// 少一种分级就编译不过
export const SAFETY_DOT_CLASS: { [K in DebugActionSafety]: string } = {
    read_only: 'bg-success',
    side_effect: 'bg-warning',
    dangerous: 'bg-danger',
};

/** 徽章（shared/ui 的 Badge）用的色调，和色点同一套颜色 */
export const SAFETY_TONE: { [K in DebugActionSafety]: 'success' | 'warning' | 'danger' } = {
    read_only: 'success',
    side_effect: 'warning',
    dangerous: 'danger',
};

export const SAFETY_LABEL: { [K in DebugActionSafety]: string } = {
    read_only: '只读',
    side_effect: '有副作用',
    dangerous: '危险',
};

export const SAFETY_TEXT: { [K in DebugActionSafety]: string } = {
    read_only: '只读：只查询，不改动任何东西',
    side_effect: '有副作用：会改动 QQ 上的状态（发消息、改设置等）',
    dangerous: '危险：后果难以撤销，发送前会再确认一次',
};

/**
 * 「仅 NC」「仅 SL」：另一个后端的目录里没有同名接口时才标。
 * `other_backend_present` 为 null 表示没有另一份目录可比，不下结论。
 */
export function onlyBackendLabel(action: DebugActionSummary, backend: BackendType): string | null {
    if (action.other_backend_present !== false) return null;
    return backend === 'napcat' ? '仅 NC' : '仅 SL';
}
