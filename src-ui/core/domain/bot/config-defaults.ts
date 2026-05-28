// BotConfig 默认值工厂 + 校验，纯函数。

import type { BotConfig } from '../../ipc/generated/domain/BotConfig';

export function createDefaultBotConfig(): BotConfig {
    return {
        bot: {
            name: '',
            QQID: 0,
            musicSignUrl: '',
            autoRestartSchedule: { enable: false, time_unit: 'h', duration: 6 },
            offlineAutoRestart: false,
            runtime_target: 'local',
            backend_type: 'napcat',
        },
        connect: {
            httpServers: [],
            httpSseServers: [],
            httpClients: [],
            websocketServers: [],
            websocketClients: [],
            plugins: [],
        },
        advanced: {
            autoStart: false,
            offlineNotice: false,
            parseMultMsg: false,
            packetServer: '',
            packetBackend: 'auto',
            enableLocalFile2Url: false,
            fileLog: false,
            consoleLog: true,
            fileLogLevel: 'debug',
            consoleLogLevel: 'info',
            o3HookMode: 1,
            bypass: { hook: false, window: false, module: false, process: false, container: false, js: false },
        },
    };
}

export type ValidationResult =
    | { ok: true }
    | { ok: false; reason: string };

/// 保存前的最低限度校验。
/// 后端 Rust 还会再做一次完整校验（`BotConfig::validate`），这里只挡住明显错误。
export function validateBotConfig(config: BotConfig): ValidationResult {
    if (!Number.isFinite(config.bot.QQID) || config.bot.QQID <= 0) {
        return { ok: false, reason: '账号 (QQ ID) 必须是一个正整数！' };
    }
    if (!config.bot.name.trim()) {
        return { ok: false, reason: '实例名称不能为空！' };
    }
    // SnowLuma HotStart 不再持久化 attach_pid，PID 由 backend 启动时自动按 qq_id 匹配。
    // 如果 qq_id 缺失上面已挡掉；这里不再有额外校验。
    return { ok: true };
}
