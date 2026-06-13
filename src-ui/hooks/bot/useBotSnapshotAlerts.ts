// Bot 列表页：把快照里的失败/踢线/崩溃从卡片 meta 挪到全局 InfoBar。
// 用边沿检测，同一 bot 同 key 顶替，避免 react-query 轮询反复弹。

import { useEffect, useRef } from 'react';
import type {
    BotActorSnapshot,
    DaemonState,
    NapCatLoginInvalidationReason,
} from '../../core/ipc/types';
import { pushInfoBar } from '../ui/globalInfoBarStore';

export type BotSnapshotAlertRow = {
    bot: BotActorSnapshot;
    displayName: string;
    invalidationReason: NapCatLoginInvalidationReason | null | undefined;
    isSnowLuma: boolean;
    snowlumaDaemonState: DaemonState | null | undefined;
    offlineAutoRestart: boolean;
};

type PrevRow = {
    lastError: string | null;
    kicked: boolean;
    crashed: boolean;
    daemonCrashed: boolean;
};

function normError(v: string | null | undefined): string | null {
    const t = v?.trim();
    return t && t.length > 0 ? t : null;
}

export function useBotSnapshotAlerts(rows: BotSnapshotAlertRow[]): void {
    const prevRef = useRef<Map<string, PrevRow>>(new Map());

    useEffect(() => {
        const prevMap = prevRef.current;
        const nextIds = new Set<string>();

        for (const row of rows) {
            const id = row.bot.bot_id;
            nextIds.add(id);
            const lastError = normError(row.bot.last_error);
            const kicked = row.invalidationReason === 'kicked';
            const crashed = row.bot.state === 'crashed';
            const daemonCrashed =
                row.isSnowLuma && row.snowlumaDaemonState === 'crashed';

            const prev = prevMap.get(id) ?? {
                lastError: null,
                kicked: false,
                crashed: false,
                daemonCrashed: false,
            };

            const label = row.displayName;

            if (lastError && lastError !== prev.lastError) {
                pushInfoBar({
                    key: `bot-last-error:${id}`,
                    tone: 'danger',
                    title: `Bot 异常 · ${label}`,
                    content: lastError,
                });
            }

            if (kicked && !prev.kicked) {
                pushInfoBar({
                    key: `bot-kicked:${id}`,
                    tone: 'warning',
                    title: '账号已被踢',
                    content: row.offlineAutoRestart
                        ? `${label} 被踢，正在自动重启`
                        : `${label} 被踢，请手动重启`,
                    autoDismissMs: row.offlineAutoRestart ? 5000 : 0,
                });
            }

            if (crashed && !prev.crashed) {
                pushInfoBar({
                    key: `bot-crashed:${id}`,
                    tone: 'danger',
                    title: `Bot 已崩溃 · ${label}`,
                    content: lastError ?? '进程异常退出，请查看日志',
                });
            }

            if (daemonCrashed && !prev.daemonCrashed) {
                pushInfoBar({
                    key: `bot-daemon-crashed:${id}`,
                    tone: 'danger',
                    title: `SnowLuma daemon 崩溃 · ${label}`,
                    content: '请查看日志或重启相关 Bot',
                });
            }

            prevMap.set(id, {
                lastError,
                kicked,
                crashed,
                daemonCrashed,
            });
        }

        for (const key of Array.from(prevMap.keys())) {
            if (!nextIds.has(key)) prevMap.delete(key);
        }
    }, [rows]);
}