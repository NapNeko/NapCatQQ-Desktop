// 「+」菜单里能开的：本机探到的 shell、已添加的远端主机。

import { useQuery } from '@tanstack/react-query';
import { terminalService } from '../../core/services/terminal.service';
import { serverService } from '../../core/services/server.service';

export function useTerminalLaunchOptions(enabled: boolean) {
    const shells = useQuery({
        queryKey: ['terminal', 'local-shells'],
        queryFn: () => terminalService.localShells(),
        staleTime: 5 * 60_000,
        enabled,
    });
    const servers = useQuery({
        queryKey: ['servers'],
        queryFn: () => serverService.list(),
        staleTime: 30_000,
        enabled,
    });
    return {
        shells: shells.data ?? [],
        servers: (servers.data ?? []).map((s) => ({ id: s.id, label: s.name?.trim() || s.host })),
    };
}
