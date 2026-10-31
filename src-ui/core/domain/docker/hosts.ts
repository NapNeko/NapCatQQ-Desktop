import type { Os } from '../../ipc/types';

/** Windows / macOS 保持只探测远端；Linux 桌面还管理当前用户的本机容器。 */
export function dockerProbeHosts(desktopOs: Os, serverIds: readonly string[]): string[] {
    const remote = serverIds.map((id) => `remote:${id}`);
    return desktopOs === 'linux' ? ['local', ...remote] : remote;
}
