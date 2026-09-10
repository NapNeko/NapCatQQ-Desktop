// AstrBot 窄配置的前端校验 / 默认值。规则与后端 `AstrBotInstanceConfig::validate` 对齐。

import type { AppConfigIssue, AstrBotInstanceConfig } from '../../ipc/types';

export function astrbotDefaultConfig(port: number): AstrBotInstanceConfig {
    return {
        onebot: {
            id: '',
            enable: true,
            ws_reverse_host: '0.0.0.0',
            ws_reverse_port: port || 6199,
            ws_reverse_token: '',
        },
        dashboard_port: 6185,
        other_platforms: [],
        claimed: false,
    };
}

export function validateAstrBotConfig(cfg: AstrBotInstanceConfig): AppConfigIssue[] {
    const out: AppConfigIssue[] = [];
    if (!Number.isInteger(cfg.onebot.ws_reverse_port) || cfg.onebot.ws_reverse_port < 1) {
        out.push({ path: 'onebot/ws_reverse_port', message: '端口不能为 0' });
    }
    if (!cfg.onebot.ws_reverse_host.trim()) {
        out.push({ path: 'onebot/ws_reverse_host', message: '主机不能为空' });
    }
    if (cfg.dashboard_port > 0 && cfg.onebot.ws_reverse_port === cfg.dashboard_port) {
        out.push({
            path: 'onebot/ws_reverse_port',
            message: `OneBot 口不能与 WebUI 口相同（${cfg.dashboard_port}）`,
        });
    }
    return out;
}

export function astrbotLinkInputsChanged(a: AstrBotInstanceConfig, b: AstrBotInstanceConfig): boolean {
    return (
        a.onebot.ws_reverse_port !== b.onebot.ws_reverse_port ||
        a.onebot.ws_reverse_token !== b.onebot.ws_reverse_token
    );
}
