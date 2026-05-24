// WebUI 按钮可用性 + URL 拼装，纯函数。

import type { Flavor } from '../bot/flavor';
import type { DaemonState } from '../../ipc/generated/DaemonState';

export interface NapcatWebuiBinding {
    port: number;
    token: string;
}

/// NapCat：等 napcat_webui_available 事件聚合到 (port, token) 才可用。
export function isNapcatWebuiAvailable(
    binding: NapcatWebuiBinding | null | undefined,
): boolean {
    return !!binding && typeof binding.port === 'number' && !!binding.token;
}

/// 拼 NapCat WebUI 自动登录链接（带 token 查询参数）。
export function buildNapcatWebuiUrl(binding: NapcatWebuiBinding): string {
    return `http://127.0.0.1:${binding.port}/webui?token=${encodeURIComponent(binding.token)}`;
}

/// SnowLuma：daemon Ready 才允许打开 WebUI。
export function isSnowlumaWebuiAvailable(
    daemonState: DaemonState | null | undefined,
): boolean {
    return daemonState === 'ready';
}

/// 综合判断：根据 flavor 选 NapCat / SnowLuma 的可用性策略。
export function isWebuiAvailable(args: {
    flavor: Flavor | null | undefined;
    napcat?: NapcatWebuiBinding | null;
    snowlumaDaemonState?: DaemonState | null;
}): boolean {
    if (args.flavor === 'snowluma') {
        return isSnowlumaWebuiAvailable(args.snowlumaDaemonState);
    }
    return isNapcatWebuiAvailable(args.napcat);
}

export function webuiTooltip(args: {
    flavor: Flavor | null | undefined;
    available: boolean;
}): string {
    if (args.available) {
        return args.flavor === 'snowluma'
            ? '在浏览器中打开 SnowLuma WebUI（密码会复制到剪贴板）'
            : '在浏览器中打开 NapCat WebUI';
    }
    return args.flavor === 'snowluma'
        ? 'WebUI 链接将在 SnowLuma daemon 就绪后可用'
        : 'WebUI 链接将在 Bot 启动后可用';
}
