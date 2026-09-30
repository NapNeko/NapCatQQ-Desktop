// 事件接收器状态的展示文案：顶栏「正在接收」弹出层和右栏工具条共用。

import type { DebugReceiverInfo } from '../../ipc/generated/debug/DebugReceiverInfo';
import type { DebugReceiverState } from '../../ipc/generated/debug/DebugReceiverState';

type Tone = 'success' | 'warning' | 'danger' | 'neutral';

export function receiverStateCopy(state: DebugReceiverState): { text: string; tone: Tone } {
    switch (state.state) {
        case 'connecting':
            return { text: '连接中', tone: 'neutral' };
        case 'connected':
            return { text: '已连接', tone: 'success' };
        case 'reconnecting': {
            const secs = Math.max(1, Math.ceil(state.retry_in_ms / 1000));
            return { text: `重连中（第 ${state.attempt} 次，${secs} 秒后）`, tone: 'warning' };
        }
        case 'stopped':
            return { text: state.reason ? `已停止：${state.reason}` : '已停止', tone: 'neutral' };
        default: {
            const _exhaustive: never = state;
            return _exhaustive;
        }
    }
}

/** 还在收（没停）的接收器；顶栏「正在接收 N 个 Bot」的 N 就是它的个数 */
export function activeReceivers(list: readonly DebugReceiverInfo[] | undefined): DebugReceiverInfo[] {
    return (list ?? []).filter((r) => r.state.state !== 'stopped');
}
