// 远端主机卡徽标语义（对齐 botCardPresentation）。

import type { ServerProfile } from '../../core/ipc/generated/domain/ServerProfile';
import type { ServerState } from '../../core/ipc/generated/domain/ServerState';

export type ServerBadgeSpec = {
    tone: 'neutral' | 'brand' | 'success' | 'warning' | 'danger' | 'info';
    label: string;
    dot?: boolean;
};

export function serverLifecycleBadge(state: ServerState): ServerBadgeSpec {
    switch (state) {
        case 'connected':
            return { tone: 'success', label: '在线', dot: true };
        case 'connecting':
            return { tone: 'brand', label: '连接中', dot: true };
        case 'failed':
            return { tone: 'danger', label: '无法连接', dot: true };
        case 'disconnected':
        default:
            return { tone: 'neutral', label: '未测试', dot: true };
    }
}

/**
 * 卡片上唯一一条连接异常说明。
 *
 * 状态（无法连接 / 在线）交给底栏徽标，这里只补「为什么失败、失败了几次」，
 * 用次级色一行显示；不再用第二段红字重复「连接中断」。失败原因来自
 * ConnectionHealth.lastFailureReason，SSH 报错原文放 title 里。
 */
export function serverConnectionNote(
    server: Pick<ServerProfile, 'state' | 'health'>,
): { text: string; title: string } | null {
    if (server.state !== 'failed') return null;
    const failures = server.health?.consecutiveFailures ?? 0;
    const reason = server.health?.lastFailureReason?.trim() ?? '';
    const retryHint = '可点卡片底部的 Wi-Fi 图标重新测试连接。';
    if (reason && failures > 0) {
        return {
            text: `${failures} 次失败 · ${reason}`,
            title: `${reason}\n${retryHint}`,
        };
    }
    if (reason) return { text: reason, title: `${reason}\n${retryHint}` };
    if (failures > 0) return { text: `已连续失败 ${failures} 次`, title: retryHint };
    return null;
}
