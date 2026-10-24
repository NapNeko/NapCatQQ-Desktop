// IPC 响应和事件可能乱序到达，较旧的 revision 不能覆盖新状态。
import type { BotActorSnapshot } from '../../core/ipc/types';

export function updateBotSnapshots(
    current: BotActorSnapshot[] | undefined,
    incoming: BotActorSnapshot,
): BotActorSnapshot[] {
    if (!current?.some((snapshot) => snapshot.bot_id === incoming.bot_id)) {
        return [...(current ?? []), incoming];
    }
    return current.map((snapshot) =>
        snapshot.bot_id === incoming.bot_id && incoming.revision >= snapshot.revision
            ? incoming
            : snapshot,
    );
}

export function mergeBotSnapshotList(
    current: BotActorSnapshot[] | undefined,
    incoming: BotActorSnapshot[],
): BotActorSnapshot[] {
    const known = new Map(current?.map((snapshot) => [snapshot.bot_id, snapshot]));
    return incoming.map((snapshot) => {
        const previous = known.get(snapshot.bot_id);
        return previous && previous.revision > snapshot.revision ? previous : snapshot;
    });
}
