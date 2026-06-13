// BotConfig 默认值工厂 + 校验，纯函数。

import type { BotConfig } from '../../ipc/generated/domain/BotConfig';
import type { StatusCommandConfig } from '../../ipc/generated/domain/StatusCommandConfig';
import { RUNTIME_TARGET_REMOTE_PLACEHOLDER } from './runtime-target';

export const defaultStatusCommandConfig = (): StatusCommandConfig => ({
    enabled: true,
    swallow: false,
    cooldownSeconds: 5,
});

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
            deploymentType: 'native',
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
        statusCommand: undefined,
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
    // 运行宿主选了"远程"但还没选具体机器：runtime_target 仍是占位 'remote'
    // （真实值应是 server_id）。挡住,否则后端解析 host 会失败。
    if (config.bot.runtime_target === RUNTIME_TARGET_REMOTE_PLACEHOLDER) {
        return { ok: false, reason: '请选择一台具体的远程主机！' };
    }
    // 本机(Windows)不支持 Docker:Docker Desktop 安装链路太麻烦,本机只走直接运行。
    // Docker 启动方式只允许配合远程 SSH 主机。
    if (config.bot.deploymentType === 'docker' && config.bot.runtime_target === 'local') {
        return { ok: false, reason: '本机暂不支持 Docker 部署，请改用「直接运行」，或把运行宿主切换为远程 SSH 主机。' };
    }
    // SnowLuma HotStart 不再持久化 attach_pid，PID 由 backend 启动时自动按 qq_id 匹配。
    // 如果 qq_id 缺失上面已挡掉；这里不再有额外校验。
    return { ok: true };
}
