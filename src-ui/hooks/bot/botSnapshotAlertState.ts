// Bot 快照告警的模块级记忆：跨路由卸载 hook 也不丢边沿状态；用户手动关闭的持久告警记入抑制集。

export type BotSnapshotPrevRow = {
    lastError: string | null;
    kicked: boolean;
    crashed: boolean;
    daemonCrashed: boolean;
};

const EMPTY_PREV: BotSnapshotPrevRow = {
    lastError: null,
    kicked: false,
    crashed: false,
    daemonCrashed: false,
};

const prevByBotId = new Map<string, BotSnapshotPrevRow>();

/** 用户点 InfoBar 关闭后，在对应 Bot 状态恢复前不再自动 push 同 key。 */
const userDismissedAlertKeys = new Set<string>();

export function getBotSnapshotPrev(botId: string): BotSnapshotPrevRow {
    return prevByBotId.get(botId) ?? EMPTY_PREV;
}

export function setBotSnapshotPrev(botId: string, row: BotSnapshotPrevRow): void {
    prevByBotId.set(botId, row);
}

export function pruneBotSnapshotPrev(activeBotIds: Set<string>): void {
    for (const id of Array.from(prevByBotId.keys())) {
        if (!activeBotIds.has(id)) prevByBotId.delete(id);
    }
}

export function isBotSnapshotAlertSuppressed(alertKey: string): boolean {
    return userDismissedAlertKeys.has(alertKey);
}

export function suppressBotSnapshotAlert(alertKey: string): void {
    userDismissedAlertKeys.add(alertKey);
}

export function clearBotSnapshotAlertSuppression(alertKey: string): void {
    userDismissedAlertKeys.delete(alertKey);
}

/** 测试用。 */
export function _resetBotSnapshotAlertState(): void {
    prevByBotId.clear();
    userDismissedAlertKeys.clear();
}