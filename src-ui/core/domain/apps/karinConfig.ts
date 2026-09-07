// Karin 实例配置的前端侧知识：常量 / 默认值 / 即时校验 / 新条目工厂。
// 真相在 crates/ncd-appframework/src/karin/config.rs，这里的校验只做「保存前立刻提示」，
// 后端仍会再校验一遍（ConfigInvalid 按 path 回填到字段）。

import type {
    AppConfigIssue,
    KarinEnv,
    KarinInstanceConfig,
    KarinOneBotHttpServer,
    KarinOneBotWsClient,
    KarinRenderHttpServer,
    KarinRenderWsClient,
    KarinScopeRule,
} from '../../ipc/types';

export const KARIN_LOG_LEVELS = [
    'all',
    'trace',
    'debug',
    'mark',
    'info',
    'warn',
    'error',
    'fatal',
    'off',
] as const;
export const KARIN_RUNTIMES = ['node', 'pm2', 'tsx'] as const;
export const KARIN_NODE_ENVS = ['development', 'production', 'test'] as const;

export const KARIN_SYSTEM_ENV_KEYS = [
    'HTTP_ENABLE',
    'HTTP_PORT',
    'HTTP_HOST',
    'HTTP_AUTH_KEY',
    'WS_SERVER_AUTH_KEY',
    'REDIS_ENABLE',
    'PM2_RESTART',
    'TSX_WATCH',
    'LOG_LEVEL',
    'LOG_DAYS_TO_KEEP',
    'LOG_MAX_LOG_SIZE',
    'LOG_FNC_COLOR',
    'LOG_MAX_CONNECTIONS',
    'FFMPEG_PATH',
    'FFPROBE_PATH',
    'FFPLAY_PATH',
    'RUNTIME',
    'NODE_ENV',
] as const;

/** 群 / 私聊响应模式（Karin 固定语义） */
export const KARIN_RULE_MODES: ReadonlyArray<{ value: number; label: string; hint: string }> = [
    { value: 0, label: '所有消息', hint: '不限制' },
    { value: 1, label: '仅 @ 机器人', hint: '必须 @Bot 才响应' },
    { value: 2, label: '仅管理员', hint: '只响应 master / admin' },
    { value: 3, label: '仅别名', hint: '消息须以别名开头' },
    { value: 4, label: '别名或 @', hint: '别名开头或 @Bot 任一满足' },
    { value: 5, label: '管理员无限制', hint: '管理员不受限，其他人需别名或 @' },
    { value: 6, label: '仅主人', hint: '只响应 master' },
];

export const KARIN_GROUP_RULE_KEYS = [
    'default',
    'global',
    'Bot:selfId',
    'Bot:selfId:groupId',
    'Bot:selfId:guildId',
    'Bot:selfId:guildId:channelId',
] as const;
export const KARIN_PRIVATE_RULE_KEYS = [
    'default',
    'global',
    'Bot:selfId',
    'Bot:selfId:userId',
] as const;

export function newGroupRule(key = 'Bot:selfId:groupId'): KarinScopeRule {
    return {
        key,
        inherit: true,
        cd: 0,
        userCD: 0,
        mode: 0,
        alias: [],
        enable: [],
        disable: [],
        member_enable: [],
        member_disable: [],
    };
}

export function newPrivateRule(key = 'Bot:selfId:userId'): KarinScopeRule {
    return { key, inherit: true, cd: 0, mode: 0, alias: [], enable: [], disable: [] };
}

export function newOneBotWsClient(): KarinOneBotWsClient {
    return { enable: false, url: 'ws://127.0.0.1:7778', token: '' };
}

export function newOneBotHttpServer(): KarinOneBotHttpServer {
    return {
        enable: false,
        self_id: 'default',
        url: 'http://127.0.0.1:6099',
        token: '',
        api_token: '',
        post_token: '',
    };
}

export function newRenderWsClient(): KarinRenderWsClient {
    return {
        enable: false,
        url: 'ws://127.0.0.1:7005',
        token: '123456',
        isSnapka: false,
        reconnectTime: 5000,
        heartbeatTime: 30000,
    };
}

export function newRenderHttpServer(): KarinRenderHttpServer {
    return { enable: false, url: 'http://127.0.0.1:7005', token: '123456', isSnapka: false };
}

/** 与 Karin `default.ts` / 后端 `upstream_default()` 一致（mock 种子，也是「恢复默认」的参考） */
export function karinDefaultConfig(port = 7777): KarinInstanceConfig {
    const scope = () => ({
        enable: true,
        enable_list: [],
        disable_list: [],
        log_enable_list: [],
        log_disable_list: [],
    });
    return {
        env: {
            http_enable: true,
            http_port: port,
            http_host: '0.0.0.0',
            http_auth_key: '',
            ws_server_auth_key: '',
            redis_enable: true,
            pm2_restart: true,
            tsx_watch: false,
            log_level: 'info',
            log_days_to_keep: 7,
            log_max_log_size: 0,
            log_fnc_color: '#E1D919',
            log_max_connections: 5,
            ffmpeg_path: '',
            ffprobe_path: '',
            ffplay_path: '',
            runtime: 'node',
            node_env: 'production',
            comments: {},
            custom: [],
        },
        config: {
            master: ['console'],
            admin: [],
            user: { enable_list: [], disable_list: [] },
            friend: scope(),
            group: scope(),
            directs: scope(),
            guilds: scope(),
            channels: scope(),
        },
        adapter: {
            console: { isLocal: true, token: '', host: '' },
            onebot: {
                ws_server: { enable: true, timeout: 120 },
                ws_client: [newOneBotWsClient()],
                http_server: [newOneBotHttpServer()],
            },
        },
        groups: KARIN_GROUP_RULE_KEYS.map((k) => newGroupRule(k)),
        privates: KARIN_PRIVATE_RULE_KEYS.map((k) => newPrivateRule(k)),
        render: {
            ws_server: { enable: true },
            ws_client: [newRenderWsClient()],
            http_server: [newRenderHttpServer()],
        },
        redis: { url: 'redis://127.0.0.1:6379', username: '', password: '', database: 0 },
    };
}

const ENV_KEY_RE = /^[A-Za-z_][A-Za-z0-9_]*$/;
const isWs = (u: string) => /^wss?:\/\//.test(u.trim());
const isHttp = (u: string) => /^https?:\/\//.test(u.trim());

function validateRules(rules: KarinScopeRule[], root: string, out: AppConfigIssue[]) {
    const seen = new Set<string>();
    rules.forEach((r, i) => {
        const key = r.key.trim();
        if (!key) out.push({ path: `${root}/${i}/key`, message: '规则键不能为空' });
        else if (seen.has(key)) out.push({ path: `${root}/${i}/key`, message: `规则键 ${key} 重复` });
        else seen.add(key);
        if (!Number.isInteger(r.mode) || r.mode < 0 || r.mode > 6) {
            out.push({ path: `${root}/${i}/mode`, message: 'mode 须在 0–6' });
        }
    });
}

/** 与后端 `KarinInstanceConfig::validate` 同规则、同 path，保证前后端报错落在同一个字段 */
export function validateKarinConfig(cfg: KarinInstanceConfig): AppConfigIssue[] {
    const out: AppConfigIssue[] = [];
    const env = cfg.env;
    if (!Number.isInteger(env.http_port) || env.http_port < 1 || env.http_port > 65535) {
        out.push({ path: 'env/http_port', message: '端口需在 1–65535' });
    }
    if (!(KARIN_LOG_LEVELS as readonly string[]).includes(env.log_level)) {
        out.push({ path: 'env/log_level', message: `日志等级须是 ${KARIN_LOG_LEVELS.join(' / ')} 之一` });
    }
    if (!(KARIN_RUNTIMES as readonly string[]).includes(env.runtime)) {
        out.push({ path: 'env/runtime', message: `运行器须是 ${KARIN_RUNTIMES.join(' / ')} 之一` });
    }
    if (!(KARIN_NODE_ENVS as readonly string[]).includes(env.node_env)) {
        out.push({ path: 'env/node_env', message: `NODE_ENV 须是 ${KARIN_NODE_ENVS.join(' / ')} 之一` });
    }
    const seen = new Set<string>();
    env.custom.forEach((e, i) => {
        const path = `env/custom/${i}/key`;
        const key = e.key.trim();
        if (!key) return out.push({ path, message: '键不能为空' });
        if (!ENV_KEY_RE.test(key)) {
            return out.push({ path, message: '键只能含字母、数字、下划线，且不能以数字开头' });
        }
        if ((KARIN_SYSTEM_ENV_KEYS as readonly string[]).includes(key)) {
            return out.push({ path, message: `${key} 是系统键，请在上方对应字段修改` });
        }
        if (seen.has(key)) return out.push({ path, message: `键 ${key} 重复` });
        seen.add(key);
        return undefined;
    });

    if (!(cfg.adapter.onebot.ws_server.timeout > 0)) {
        out.push({ path: 'adapter/onebot/ws_server/timeout', message: '超时须大于 0 秒' });
    }
    cfg.adapter.onebot.ws_client.forEach((c, i) => {
        if (!isWs(c.url)) out.push({ path: `adapter/onebot/ws_client/${i}/url`, message: '须以 ws:// 或 wss:// 开头' });
    });
    cfg.adapter.onebot.http_server.forEach((s, i) => {
        if (!isHttp(s.url)) out.push({ path: `adapter/onebot/http_server/${i}/url`, message: '须以 http:// 或 https:// 开头' });
        if (!s.self_id.trim()) out.push({ path: `adapter/onebot/http_server/${i}/self_id`, message: 'self_id 不能为空' });
    });

    validateRules(cfg.groups, 'groups', out);
    validateRules(cfg.privates, 'privates', out);

    cfg.render.ws_client.forEach((c, i) => {
        if (!isWs(c.url)) out.push({ path: `render/ws_client/${i}/url`, message: '须以 ws:// 或 wss:// 开头' });
    });
    cfg.render.http_server.forEach((s, i) => {
        if (!isHttp(s.url)) out.push({ path: `render/http_server/${i}/url`, message: '须以 http:// 或 https:// 开头' });
    });

    const redis = cfg.redis.url.trim();
    if (!(redis.startsWith('redis://') || redis.startsWith('rediss://'))) {
        out.push({ path: 'redis/url', message: '须以 redis:// 或 rediss:// 开头' });
    }
    return out;
}

export function issuesByPath(issues: AppConfigIssue[]): Record<string, string> {
    const map: Record<string, string> = {};
    for (const i of issues) if (!(i.path in map)) map[i.path] = i.message;
    return map;
}

/** 对接依赖的两把钥匙是否变了（保存前提示「会重新对接」） */
export function karinLinkInputsChanged(a: KarinEnv, b: KarinEnv): boolean {
    return a.http_port !== b.http_port || a.ws_server_auth_key !== b.ws_server_auth_key;
}

/** 把 .env 类型化视图渲染回 dotenv 文本（mock / 预览用；真写盘在后端保注释） */
export function karinEnvToDotenv(env: KarinEnv): string {
    const b = (v: boolean) => (v ? 'true' : 'false');
    const pairs: Array<[string, string]> = [
        ['HTTP_ENABLE', b(env.http_enable)],
        ['HTTP_PORT', String(env.http_port)],
        ['HTTP_HOST', env.http_host],
        ['HTTP_AUTH_KEY', env.http_auth_key],
        ['WS_SERVER_AUTH_KEY', env.ws_server_auth_key],
        ['REDIS_ENABLE', b(env.redis_enable)],
        ['PM2_RESTART', b(env.pm2_restart)],
        ['TSX_WATCH', b(env.tsx_watch)],
        ['LOG_LEVEL', env.log_level],
        ['LOG_DAYS_TO_KEEP', String(env.log_days_to_keep)],
        ['LOG_MAX_LOG_SIZE', String(env.log_max_log_size)],
        ['LOG_FNC_COLOR', env.log_fnc_color],
        ['LOG_MAX_CONNECTIONS', String(env.log_max_connections)],
        ['FFMPEG_PATH', env.ffmpeg_path],
        ['FFPROBE_PATH', env.ffprobe_path],
        ['FFPLAY_PATH', env.ffplay_path],
        ['RUNTIME', env.runtime],
        ['NODE_ENV', env.node_env],
    ];
    const quote = (v: string) => (/[\s#"]/.test(v) ? `"${v.replace(/"/g, '\\"')}"` : v);
    const lines: string[] = [];
    for (const [k, v] of pairs) {
        const c = env.comments[k];
        if (c) lines.push(`# ${c}`);
        lines.push(`${k}=${quote(v)}`);
    }
    if (env.custom.length) lines.push('');
    for (const e of env.custom) {
        if (e.comment) lines.push(`# ${e.comment}`);
        lines.push(`${e.key}=${quote(e.value)}`);
    }
    return `${lines.join('\n')}\n`;
}
