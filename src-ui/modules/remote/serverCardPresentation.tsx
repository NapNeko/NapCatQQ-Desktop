// 远端主机卡徽标语义（对齐 botCardPresentation）。

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
            return { tone: 'danger', label: '远端不可达', dot: true };
        case 'disconnected':
        default:
            return { tone: 'neutral', label: '未测试', dot: true };
    }
}

export type ServerStatusTone =
    | 'success'
    | 'warning'
    | 'danger'
    | 'brand'
    | 'neutral';

export function serverStatusLineToneClass(tone: ServerStatusTone): string {
    switch (tone) {
        case 'success':
            return 'text-success';
        case 'warning':
            return 'text-warning';
        case 'danger':
            return 'text-danger';
        case 'brand':
            return 'text-brand';
        case 'neutral':
        default:
            return 'text-text-secondary';
    }
}

export function serverStatusLine(
    state: ServerState,
    isTesting: boolean,
): { text: string; tone: ServerStatusTone } | null {
    if (isTesting) return { text: '正在测试连接…', tone: 'brand' };
    switch (state) {
        case 'connected':
            return { text: '服务器在线，可以部署组件', tone: 'success' };
        case 'connecting':
            return { text: '正在连接…', tone: 'brand' };
        case 'failed':
            return { text: '远端主机不可达，请检查网络或凭据', tone: 'danger' };
        case 'disconnected':
        default:
            return null; // 未测试时不显示状态行，避免视觉噪音
    }
}