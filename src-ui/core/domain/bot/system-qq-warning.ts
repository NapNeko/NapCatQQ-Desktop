// 本机 Bot 要用用户自己装的 QQ 时，启动前提醒一次
import type { BotConfig } from '../../ipc/generated/domain/BotConfig';
import { getRuntimeRequirement } from './runtime-gate';

/** 本机直接运行、由桌面端拉起 QQ 的 Bot 才用到本机 QQ；SnowLuma 热启动接的是用户自己开着的 QQ，不问 */
export function launchesLocalQq(config: BotConfig): boolean {
    if (getRuntimeRequirement(config)?.kind !== 'local-direct') return false;
    if (config.bot.backend_type === 'snowluma') {
        return config.bot.snowlumaStartMode?.mode !== 'hot_start';
    }
    return true;
}

const DISMISS_KEY = 'ncd.bot.systemQqWarning.dismissed.v1';

export function isSystemQqWarningDismissed(): boolean {
    try {
        return localStorage.getItem(DISMISS_KEY) === '1';
    } catch {
        return false;
    }
}

export function dismissSystemQqWarning(): void {
    try {
        localStorage.setItem(DISMISS_KEY, '1');
    } catch {
        // 存不下就下次再提醒
    }
}
