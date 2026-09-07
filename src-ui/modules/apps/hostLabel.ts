// host_id（`local` / `remote:<server_id>`）→ 用户可读位置名。

import { runtimeTargetDisplayLabel } from '../../core/domain/bot/runtime-target';

type ServerNameSource = {
    id: string;
    name?: string | null;
    host?: string | null;
};

export function hostIdDisplayLabel(hostId: string, servers: readonly ServerNameSource[]): string {
    if (hostId === 'local') return '本机';
    return runtimeTargetDisplayLabel(hostId, servers);
}
