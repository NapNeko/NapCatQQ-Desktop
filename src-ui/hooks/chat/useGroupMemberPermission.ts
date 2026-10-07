import { useCallback, useSyncExternalStore } from 'react';
import type { DebugTarget } from '../../core/ipc/generated/debug/DebugTarget';
import { groupMemberPermissions } from '../../core/services/group-member-permissions.service';

export function useGroupMemberPermission(
    target: DebugTarget,
    groupId: string,
    memberId: string,
    enabled: boolean,
) {
    const snapshot = useCallback(
        () => (enabled ? groupMemberPermissions.peek(target, groupId, memberId) : undefined),
        [target.bot_id, target.qq_id, target.backend, groupId, memberId, enabled],
    );
    return useSyncExternalStore(groupMemberPermissions.subscribe, snapshot, snapshot);
}
