// NoneBot2 核心 / OneBot v11 配置的前端校验 / 默认值。规则与后端 `NoneBot2InstanceConfig::validate` 对齐。

import { normalizeNoneBot2Duration } from './nonebot2Duration';
import type { ConfigFormSpec } from './appConfigForm';
import type { AppConfigIssue, NoneBot2InstanceConfig } from '../../ipc/types';

export const NONEBOT2_LOG_LEVELS = [
    'TRACE',
    'DEBUG',
    'INFO',
    'WARNING',
    'ERROR',
    'CRITICAL',
] as const;

export const NONEBOT2_SYSTEM_ENV_KEYS = new Set([
    'ENVIRONMENT',
    'HOST',
    'PORT',
    'DRIVER',
    'LOG_LEVEL',
    'API_TIMEOUT',
    'SESSION_EXPIRE_TIMEOUT',
    'SUPERUSERS',
    'NICKNAME',
    'COMMAND_START',
    'COMMAND_SEP',
    'ONEBOT_ACCESS_TOKEN',
    'ONEBOT_SECRET',
    'ONEBOT_WS_URLS',
    'ONEBOT_API_ROOTS',
    'ONEBOT_V11_ACCESS_TOKEN',
    'ONEBOT_V11_SECRET',
    'ONEBOT_V11_WS_URLS',
    'ONEBOT_V11_API_ROOTS',
]);

/** 内置驱动与 OneBot v12 独立命名空间；按需加入，避免为未使用的驱动写入配置。 */
export const NONEBOT2_ENV_PRESETS = [
    {
        key: 'FASTAPI_OPENAPI_URL',
        value: '/openapi.json',
        comment: 'FastAPI OpenAPI 地址（缺省关闭）',
    },
    { key: 'FASTAPI_DOCS_URL', value: '/docs', comment: 'FastAPI Swagger 地址（缺省关闭）' },
    { key: 'FASTAPI_REDOC_URL', value: '/redoc', comment: 'FastAPI ReDoc 地址（缺省关闭）' },
    {
        key: 'FASTAPI_INCLUDE_ADAPTER_SCHEMA',
        value: 'true',
        comment: '在 OpenAPI 中包含适配器路由',
    },
    { key: 'FASTAPI_RELOAD', value: 'false', comment: 'FastAPI 冷重载' },
    { key: 'FASTAPI_RELOAD_DIRS', value: '[]', comment: 'FastAPI 重载监控目录（JSON 数组）' },
    { key: 'FASTAPI_RELOAD_DELAY', value: '0.25', comment: 'FastAPI 重载延迟（秒）' },
    { key: 'FASTAPI_RELOAD_INCLUDES', value: '[]', comment: 'FastAPI 监听 glob（JSON 数组）' },
    { key: 'FASTAPI_RELOAD_EXCLUDES', value: '[]', comment: 'FastAPI 排除 glob（JSON 数组）' },
    { key: 'FASTAPI_EXTRA', value: '{}', comment: '传递给 FastAPI 的参数（JSON 对象）' },
    { key: 'QUART_RELOAD', value: 'false', comment: 'Quart 冷重载' },
    { key: 'QUART_RELOAD_DIRS', value: '[]', comment: 'Quart 重载监控目录（JSON 数组）' },
    { key: 'QUART_RELOAD_DELAY', value: '0.25', comment: 'Quart 重载延迟（秒）' },
    { key: 'QUART_RELOAD_INCLUDES', value: '[]', comment: 'Quart 监听 glob（JSON 数组）' },
    { key: 'QUART_RELOAD_EXCLUDES', value: '[]', comment: 'Quart 排除 glob（JSON 数组）' },
    { key: 'QUART_EXTRA', value: '{}', comment: '传递给 Quart 的参数（JSON 对象）' },
    { key: 'ONEBOT_V12_ACCESS_TOKEN', value: '', comment: 'OneBot v12 授权令牌' },
    { key: 'ONEBOT_V12_WS_URLS', value: '[]', comment: 'OneBot v12 正向 WS 地址（JSON 数组）' },
    { key: 'ONEBOT_V12_API_ROOTS', value: '{}', comment: 'OneBot v12 HTTP API 地址（JSON 对象）' },
    {
        key: 'ONEBOT_V12_USE_MSGPACK',
        value: 'false',
        comment: 'OneBot v12 msgpack，可填布尔值或 self_id 到布尔值的 JSON 对象',
    },
] as const;

export function appendNoneBot2EnvPreset(
    custom: NoneBot2InstanceConfig['env_prod']['custom'],
    key: string,
): NoneBot2InstanceConfig['env_prod']['custom'] {
    const preset = NONEBOT2_ENV_PRESETS.find((entry) => entry.key === key);
    if (!preset || custom.some((entry) => entry.key.toUpperCase() === key)) return custom;
    return [...custom, { ...preset }];
}

export function nonebot2DefaultConfig(port: number): NoneBot2InstanceConfig {
    return {
        env_prod: {
            host: '127.0.0.1',
            port,
            driver: '~fastapi',
            log_level: 'INFO',
            onebot_access_token: '',
            onebot_secret: '',
            onebot_ws_urls: [],
            onebot_api_roots: '{}',
            api_timeout: '30.0',
            session_expire_timeout: 'PT2M',
            superusers: [],
            nickname: [],
            command_start: ['/'],
            command_sep: ['.'],
            custom: [],
        },
    };
}

function validNoneBot2Url(value: string, schemes: string[]): boolean {
    try {
        const url = new URL(value);
        return schemes.includes(url.protocol) && !!url.hostname;
    } catch {
        return false;
    }
}

export function validateNoneBot2Config(cfg: NoneBot2InstanceConfig): AppConfigIssue[] {
    const out: AppConfigIssue[] = [];
    if (
        !Number.isInteger(cfg.env_prod.port) ||
        cfg.env_prod.port < 1 ||
        cfg.env_prod.port > 65535
    ) {
        out.push({ path: 'env_prod/port', message: '端口需在 1–65535' });
    }
    if (!cfg.env_prod.host.trim()) {
        out.push({ path: 'env_prod/host', message: 'HOST 不能为空' });
    }
    const env = cfg.env_prod;
    for (const key of [
        'host',
        'driver',
        'log_level',
        'onebot_access_token',
        'onebot_secret',
    ] as const) {
        if (/[\r\n]/.test(env[key]))
            out.push({ path: `env_prod/${key}`, message: '值不能包含换行' });
    }
    if (
        !env.api_timeout.trim() ||
        !Number.isFinite(Number(env.api_timeout)) ||
        /[\r\n]/.test(env.api_timeout)
    ) {
        out.push({ path: 'env_prod/api_timeout', message: 'API_TIMEOUT 须为有限秒数' });
    }
    if (normalizeNoneBot2Duration(env.session_expire_timeout) == null) {
        out.push({
            path: 'env_prod/session_expire_timeout',
            message: '请输入有效时长；负数请用秒数或 -P 开头的 ISO 时长',
        });
    }
    env.onebot_ws_urls.forEach((url, i) => {
        if (!validNoneBot2Url(url, ['ws:', 'wss:']) || /[\r\n]/.test(url)) {
            out.push({
                path: `env_prod/onebot_ws_urls/${i}`,
                message: '须以 ws:// 或 wss:// 开头',
            });
        }
    });
    try {
        const roots: unknown = JSON.parse(env.onebot_api_roots);
        if (
            !roots ||
            Array.isArray(roots) ||
            typeof roots !== 'object' ||
            Object.values(roots).some(
                (url) => typeof url !== 'string' || !validNoneBot2Url(url, ['http:', 'https:']),
            )
        ) {
            throw new Error('invalid roots');
        }
    } catch {
        out.push({
            path: 'env_prod/onebot_api_roots',
            message: '须为 self_id 到 HTTP / HTTPS 地址的 JSON 对象',
        });
    }
    const seen = new Set<string>();
    cfg.env_prod.custom.forEach((c, i) => {
        if (c.key !== c.key.trim() || !/^[A-Za-z_][A-Za-z0-9_]*$/.test(c.key)) {
            out.push({ path: `env_prod/custom/${i}/key`, message: '键名只能是字母数字下划线' });
        }
        const key = c.key.toUpperCase();
        if (NONEBOT2_SYSTEM_ENV_KEYS.has(key)) {
            out.push({ path: `env_prod/custom/${i}/key`, message: '请在对应系统字段中修改此键' });
        } else if (seen.has(key)) {
            out.push({ path: `env_prod/custom/${i}/key`, message: '键名重复（不区分大小写）' });
        }
        seen.add(key);
        if (c.value.includes('\n') || c.value.includes('\r')) {
            out.push({ path: `env_prod/custom/${i}/value`, message: '值不能包含换行' });
        }
    });
    return out;
}

export const NONEBOT2_CONFIG_FORM: ConfigFormSpec<'nonebot2'> = {
    framework: 'nonebot2',
    validate: validateNoneBot2Config,
    saveHint: (r) => (r.restart_required ? '改完要重启' : null),
};

export function nonebot2LinkInputsChanged(
    a: NoneBot2InstanceConfig['env_prod'],
    b: NoneBot2InstanceConfig['env_prod'],
): boolean {
    return a.port !== b.port || a.onebot_access_token !== b.onebot_access_token;
}
