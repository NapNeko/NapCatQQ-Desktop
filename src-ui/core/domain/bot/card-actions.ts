import type { BotActorSnapshot } from '../../ipc/types';
import { canStartBot, canStopBot, isBotActive } from './status';

export type BotCardActionId =
    | 'start'
    | 'stop'
    | 'webui'
    | 'novnc'
    | 'qr'
    | 'remoteQr'
    | 'retryUi'
    | 'configure'
    | 'migrate'
    | 'logs'
    | 'metrics'
    | 'debug'
    | 'terminal'
    | 'hostTerminal'
    | 'copy';

export interface BotCardAction {
    id: BotCardActionId;
    label: string;
    disabled?: boolean;
    tone?: 'brand' | 'success' | 'danger';
    pending?: boolean;
    toolbar?: { placement: 'primary' | 'more'; label?: string };
    group: 'runtime' | 'navigation' | 'clipboard';
}

export interface BotCardCapabilities {
    webui: boolean;
    webuiTooltip: string;
    novnc: boolean;
    qr: boolean;
    remoteQr: boolean;
    retryUi: boolean;
    terminal: boolean;
    debug: boolean;
}

/** 一次决定操作可用性；工具栏和右键菜单只选择布局，不再重新判断 Bot 状态。 */
export function buildBotCardActions({
    state,
    actionPending,
    transportFailed,
    isDocker,
    retryingUi,
    capabilities: cap,
}: {
    state: BotActorSnapshot['state'];
    actionPending: boolean;
    transportFailed: boolean;
    isDocker: boolean;
    retryingUi: boolean;
    capabilities: BotCardCapabilities;
}): BotCardAction[] {
    const actions: BotCardAction[] = [];
    const primary = (label?: string) => ({ placement: 'primary' as const, label });
    const more = (label?: string) => ({ placement: 'more' as const, label });
    if (isBotActive(state)) {
        actions.push({
            id: 'stop',
            label: '停止实例',
            group: 'runtime',
            tone: 'danger',
            disabled: !canStopBot(state),
            toolbar: primary('停止 Bot'),
        });
    } else if (canStartBot(state)) {
        actions.push({
            id: 'start',
            label: '启动实例',
            group: 'runtime',
            tone: 'success',
            disabled: actionPending || transportFailed,
            pending: actionPending,
            toolbar: primary(
                actionPending
                    ? '正在准备启动'
                    : transportFailed
                      ? '远端主机不可达，无法启动'
                      : '启动 Bot',
            ),
        });
    }
    if (cap.webui)
        actions.push({
            id: 'webui',
            label: '打开 WebUI',
            group: 'runtime',
            tone: 'brand',
            toolbar: primary(cap.webuiTooltip),
        });
    if (cap.novnc)
        actions.push({
            id: 'novnc',
            label: '打开 noVNC 桌面',
            group: 'runtime',
            tone: 'brand',
            toolbar: more('noVNC 桌面'),
        });
    if (cap.remoteQr)
        actions.push({
            id: 'remoteQr',
            label: '扫码登录',
            group: 'runtime',
            tone: 'brand',
            toolbar: more(),
        });
    if (cap.retryUi)
        actions.push({
            id: 'retryUi',
            label: '重建 WebUI 连接',
            group: 'runtime',
            tone: 'brand',
            disabled: retryingUi,
            toolbar: more('重建连接'),
        });
    if (cap.qr)
        actions.push({
            id: 'qr',
            label: '扫码登录',
            group: 'runtime',
            tone: 'brand',
            toolbar: primary(),
        });
    actions.push(
        { id: 'configure', label: '配置参数', group: 'navigation', toolbar: primary() },
        { id: 'migrate', label: '迁移到服务器', group: 'navigation', toolbar: more() },
        { id: 'logs', label: '查看日志', group: 'navigation', toolbar: primary() },
        { id: 'metrics', label: '运行监控', group: 'navigation', toolbar: more() },
    );
    if (cap.debug)
        actions.push({
            id: 'debug',
            label: '在调试台打开',
            group: 'navigation',
            toolbar: state === 'running' ? more('调试控制台') : undefined,
        });
    if (cap.terminal) {
        actions.push({
            id: 'terminal',
            label: isDocker ? '终端（进容器）' : '打开终端',
            group: 'navigation',
            toolbar: more(isDocker ? '终端（容器内）' : undefined),
        });
        if (isDocker)
            actions.push({
                id: 'hostTerminal',
                label: '终端（宿主机部署目录）',
                group: 'navigation',
            });
    }
    actions.push({ id: 'copy', label: '复制 QQ 号', group: 'clipboard' });
    return actions;
}

// 两种入口的排序是呈现规则，动作对象及禁用状态始终共享。
const PRIMARY_ORDER: BotCardActionId[] = ['qr', 'start', 'stop', 'webui', 'logs', 'configure'];
const MORE_ORDER: BotCardActionId[] = [
    'migrate',
    'metrics',
    'terminal',
    'novnc',
    'remoteQr',
    'debug',
    'retryUi',
];
export function toolbarActions(actions: readonly BotCardAction[], placement: 'primary' | 'more') {
    const order = placement === 'primary' ? PRIMARY_ORDER : MORE_ORDER;
    return actions
        .filter((action) => action.toolbar?.placement === placement)
        .sort((a, b) => order.indexOf(a.id) - order.indexOf(b.id));
}
