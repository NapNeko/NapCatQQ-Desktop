// 连接状态文案的唯一来源：机器人进程、账号在线、事件通道三层状态按优先级折叠成一句提示。
import type { DebugTarget } from '../../ipc/generated/debug/DebugTarget';
import type { DebugReceiverState } from '../../ipc/generated/debug/DebugReceiverState';

export function chatConnectionLabel(
    target: Pick<DebugTarget, 'running' | 'online'>,
    state: DebugReceiverState['state'],
): string {
    if (!target.running) return '机器人已停止';
    if (target.online === false) return '账号未登录';
    switch (state) {
        case 'connected':
            return '已连接';
        case 'connecting':
            return '连接中';
        case 'reconnecting':
            return '正在重连';
        default:
            return '连接已断开';
    }
}
