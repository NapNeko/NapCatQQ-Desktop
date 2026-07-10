// Bot 列表页：把快照里的失败/踢线/崩溃从卡片 meta 挪到全局 InfoBar。
// 边沿检测 + 模块级 prev（切页不丢）；用户关闭持久态条目前写入抑制，状态恢复后再弹。

import { useEffect } from 'react';
import type {
    BotActorSnapshot,
    DaemonState,
    NapCatLoginInvalidationReason,
} from '../../core/ipc/types';
import { dismissInfoBar, pushInfoBar } from '../ui/globalInfoBarStore';
import { isQqSystemDependencyError } from '../components/useQqDependencyAlerts';
import {
    clearBotSnapshotAlertSuppression,
    getBotSnapshotPrev,
    isBotSnapshotAlertSuppressed,
    pruneBotSnapshotPrev,
    setBotSnapshotPrev,
    suppressBotSnapshotAlert,
} from './botSnapshotAlertState';

export type BotSnapshotAlertRow = {
    bot: BotActorSnapshot;
    displayName: string;
    invalidationReason: NapCatLoginInvalidationReason | null | undefined;
    isSnowLuma: boolean;
    snowlumaDaemonState: DaemonState | null | undefined;
    offlineAutoRestart: boolean;
};

function normError(v: string | null | undefined): string | null {
    const t = v?.trim();
    return t && t.length > 0 ? t : null;
}

function isSnowLumaConsentError(raw: string): boolean {
    return raw.includes('SNOWLUMA_CONSENT_REQUIRED') || raw.includes('"consentRequired":true');
}

/** InfoBar 展示用：截取首行摘要，避免 Python traceback 等长文本撑爆横幅。 */
function briefError(raw: string): string {
    const lines = raw.split('\n').map((l) => l.trim()).filter(Boolean);
    if (lines.length === 0) return raw;
    const first = lines[0];
    // 从尾部找 Python/shell 报出的最终错误行（如 OSError: ...）
    let lastMeaningful: string | undefined;
    for (let i = lines.length - 1; i >= 0; i--) {
        if (/^[A-Z]\w*(Error|Exception|Failure):/i.test(lines[i]) || lines[i].startsWith('OSError:')) {
            lastMeaningful = lines[i];
            break;
        }
    }
    const summary = lastMeaningful ?? first;
    if (summary.length <= 120) return summary;
    return summary.slice(0, 117) + '...';
}

function pushIfNotSuppressed(
    alertKey: string,
    opts: Parameters<typeof pushInfoBar>[0],
): void {
    if (isBotSnapshotAlertSuppressed(alertKey)) return;
    pushInfoBar({
        ...opts,
        key: alertKey,
        onUserDismiss: () => suppressBotSnapshotAlert(alertKey),
    });
}

export function useBotSnapshotAlerts(rows: BotSnapshotAlertRow[]): void {
    useEffect(() => {
        const nextIds = new Set<string>();

        for (const row of rows) {
            const id = row.bot.bot_id;
            nextIds.add(id);
            const lastError = normError(row.bot.last_error);
            const kicked = row.invalidationReason === 'kicked';
            const crashed = row.bot.state === 'crashed';
            const daemonCrashed =
                row.isSnowLuma && row.snowlumaDaemonState === 'crashed';

            const prev = getBotSnapshotPrev(id);
            const label = row.displayName;
            const consentRequired = lastError ? isSnowLumaConsentError(lastError) : false;

            const keyLastError = `bot-last-error:${id}`;
            const keyKicked = `bot-kicked:${id}`;
            const keyCrashed = `bot-crashed:${id}`;
            const keyDaemon = `bot-daemon-crashed:${id}`;

            if (!lastError) {
                clearBotSnapshotAlertSuppression(keyLastError);
                clearBotSnapshotAlertSuppression(`bot-qq-deps:${id}`);
                dismissInfoBar(`key:${keyLastError}`);
                dismissInfoBar(`key:bot-qq-deps:${id}`);
            }
            if (!kicked) {
                clearBotSnapshotAlertSuppression(keyKicked);
                dismissInfoBar(`key:${keyKicked}`);
            }
            if (!crashed) {
                clearBotSnapshotAlertSuppression(keyCrashed);
                dismissInfoBar(`key:${keyCrashed}`);
            }
            if (!daemonCrashed) {
                clearBotSnapshotAlertSuppression(keyDaemon);
                dismissInfoBar(`key:${keyDaemon}`);
            }

            if (lastError && !consentRequired && lastError !== prev.lastError) {
                const brief = briefError(lastError);
                if (isQqSystemDependencyError(lastError)) {
                    pushIfNotSuppressed(`bot-qq-deps:${id}`, {
                        tone: 'warning',
                        title: `QQ 系统依赖缺失 · ${label}`,
                        content: `${brief} 请到「组件」页按提示一键修复。`,
                        autoDismissMs: 0,
                    });
                }
                pushIfNotSuppressed(keyLastError, {
                    tone: 'danger',
                    title: `Bot 异常 · ${label}`,
                    content: brief,
                });
            }

            if (kicked && !prev.kicked) {
                pushIfNotSuppressed(keyKicked, {
                    tone: 'warning',
                    title: '账号已被踢',
                    content: row.offlineAutoRestart
                        ? `${label} 被踢，正在自动重启`
                        : `${label} 被踢，请手动重启`,
                    autoDismissMs: row.offlineAutoRestart ? undefined : 0,
                });
            }

            if (crashed && !consentRequired && !prev.crashed) {
                pushIfNotSuppressed(keyCrashed, {
                    tone: 'danger',
                    title: `Bot 已崩溃 · ${label}`,
                    content: lastError ? briefError(lastError) : '进程异常退出，请查看日志',
                });
            }

            if (daemonCrashed && !prev.daemonCrashed) {
                pushIfNotSuppressed(keyDaemon, {
                    tone: 'danger',
                    title: `SnowLuma daemon 崩溃 · ${label}`,
                    content: '请查看日志或重启相关 Bot',
                });
            }

            setBotSnapshotPrev(id, {
                lastError,
                kicked,
                crashed,
                daemonCrashed,
            });
        }

        pruneBotSnapshotPrev(nextIds);
    }, [rows]);
}
