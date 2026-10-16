import type { RemoteWindowsInfo } from '../../core/ipc/generated/domain/RemoteWindowsInfo';

export type WindowsSessionState =
    'offline' | 'agent_unavailable' | 'waiting' | 'ready' | 'locked' | 'multiple';

export function windowsSessionState(
    info: RemoteWindowsInfo | null | undefined,
    connected: boolean,
): WindowsSessionState {
    if (!connected) return 'offline';
    if (!info?.agent?.serviceRunning) return 'agent_unavailable';
    const workers = info.agent.sessions.filter(
        (session) => session.userSid === info.userSid && session.workerReady,
    );
    if (workers.length === 0) return 'waiting';
    if (workers.length > 1) return 'multiple';
    return workers[0].locked === true ? 'locked' : 'ready';
}

export function windowsManagementLabel(
    info: RemoteWindowsInfo | null | undefined,
    connected: boolean,
): string {
    if (!connected) return '未连接';
    return info?.agent?.serviceRunning ? '托管已就绪' : '基础连接';
}

export function windowsWorkerUpdatePending(
    info: RemoteWindowsInfo | null | undefined,
    connected: boolean,
): boolean {
    return (
        connected &&
        info?.agent?.serviceRunning === true &&
        info.agent.sessions.some(
            (session) => session.userSid === info.userSid && session.workerUpdatePending,
        )
    );
}

export function canInstallWindowsAgent(
    info: RemoteWindowsInfo | null | undefined,
    connected: boolean,
    replaceRunning = false,
): boolean {
    return (
        connected &&
        info?.elevated === true &&
        (replaceRunning || info.agent?.serviceRunning !== true)
    );
}

export function registerWindowsSessionCommand(info: RemoteWindowsInfo): string {
    const quote = (value: string) => `'${value.replaceAll("'", "''")}'`;
    const root = info.managedRoot.replace(/[\\/]+$/, '');
    return `& ${quote(`${root}\\agent\\current\\ncd-agent.exe`)} register-session --root ${quote(root)}`;
}
