// NoneBot2 窄配置的前端校验 / 默认值。规则与后端 `NoneBot2InstanceConfig::validate` 对齐。

import type { AppConfigIssue, NoneBot2InstanceConfig } from '../../ipc/types';

export const NONEBOT2_LOG_LEVELS = ['TRACE', 'DEBUG', 'INFO', 'WARNING', 'ERROR', 'CRITICAL'] as const;

export function nonebot2DefaultConfig(port: number): NoneBot2InstanceConfig {
    return {
        env_prod: {
            host: '127.0.0.1',
            port,
            driver: '~fastapi',
            log_level: 'INFO',
            onebot_access_token: '',
            superusers: [],
            nickname: [],
            command_start: ['/', ''],
            command_sep: ['.', ' '],
            custom: [],
        },
    };
}

export function validateNoneBot2Config(cfg: NoneBot2InstanceConfig): AppConfigIssue[] {
    const out: AppConfigIssue[] = [];
    if (!Number.isInteger(cfg.env_prod.port) || cfg.env_prod.port < 1) {
        out.push({ path: 'env_prod/port', message: '端口不能为 0' });
    }
    if (!cfg.env_prod.host.trim()) {
        out.push({ path: 'env_prod/host', message: 'HOST 不能为空' });
    }
    cfg.env_prod.custom.forEach((c, i) => {
        if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(c.key.trim())) {
            out.push({ path: `env_prod/custom/${i}/key`, message: '键名只能是字母数字下划线' });
        }
        if (c.value.includes('\n') || c.value.includes('\r')) {
            out.push({ path: `env_prod/custom/${i}/value`, message: '值不能包含换行' });
        }
    });
    return out;
}

export function nonebot2LinkInputsChanged(
    a: NoneBot2InstanceConfig['env_prod'],
    b: NoneBot2InstanceConfig['env_prod'],
): boolean {
    return a.port !== b.port || a.onebot_access_token !== b.onebot_access_token;
}
