// 云崽（TRSS-Yunzai）配置的前端侧知识：默认值、即时校验、下拉选项、单独设置的工厂。
// 真相在 crates/ncd-appframework/src/yunzai/config.rs，这里的校验只做「保存前立刻提示」，
// 后端会再校验一遍（ConfigInvalid 按 path 回填到字段）。

import type { ConfigFormSpec } from './appConfigForm';
import type { AppConfigIssue, YunzaiGroupOverride, YunzaiInstanceConfig } from '../../ipc/types';

export const YUNZAI_LOG_LEVELS = ['trace', 'debug', 'info', 'warn', 'fatal', 'mark', 'error', 'off'] as const;

/** renderer.yaml 的 name；空串 = 按模板里有没有脚本自动挑 */
export const YUNZAI_RENDERERS: ReadonlyArray<{ value: string; label: string }> = [
    { value: '', label: '自动' },
    { value: 'puppeteer', label: 'puppeteer（浏览器）' },
    { value: 'shotium', label: 'shotium（不要浏览器）' },
];

/** group.yaml 的 onlyReplyAt */
export const YUNZAI_REPLY_MODES: ReadonlyArray<{ value: number; label: string }> = [
    { value: 0, label: '所有消息都响应' },
    { value: 1, label: '只响应 @ 和别名' },
    { value: 2, label: '非主人只响应 @ 和别名' },
];

/** group.yaml 的 addLimit：谁能用 #添加 */
export const YUNZAI_ADD_LIMITS: ReadonlyArray<{ value: number; label: string }> = [
    { value: 0, label: '所有群员' },
    { value: 1, label: '群管理员' },
    { value: 2, label: '只有主人' },
];

/** 商店分类（和后端 store.rs 给条目打的标签一致），推荐单独走「推荐」筛选 */
export const YUNZAI_STORE_CATEGORIES = ['功能', '游戏', '文游', '单 JS'] as const;

/** 只有单 JS 能单独停（改名成 .js.disabled）；目录插件云崽整个加载，后端会拒 */
export function yunzaiPluginToggleable(id: string): boolean {
    return id.endsWith('.js');
}

/** 和后端 `YunzaiInstanceConfig::upstream_default` 一致，端口换成实例口 */
export function yunzaiDefaultConfig(port: number): YunzaiInstanceConfig {
    return {
        bot: {
            log_level: 'info',
            log_length: 10000,
            log_object: true,
            plugin_load_timeout: 60,
            file_watch: true,
            update_time: 1440,
            restart_time: 0,
            update_cron: [],
            restart_cron: [],
            stop_cron: [],
            start_cron: [],
            cache_group_member: true,
            online_msg_exp: 1440,
            file_to_url_time: 1,
            slash_to_hash: true,
            chromium_path: '',
            puppeteer_ws: '',
            proxy_address: '',
        },
        other: {
            auto_friend: 1,
            auto_group: 0,
            auto_quit: 50,
            master_qq: [],
            master: [],
            disable_private: false,
            disable_msg: '私聊功能已禁用，仅支持发送cookie，抽卡记录链接，记录日志文件',
            disable_adopt: ['stoken'],
            white_group: [],
            white_user: [],
            black_group: [],
            black_user: [],
        },
        group: {
            default: {
                group_cd: 500,
                single_cd: 2000,
                only_reply_at: 0,
                bot_alias: ['云崽', '云宝'],
                add_limit: 0,
                add_private: 1,
                add_reply: 1,
                add_at: 0,
                add_recall: 60,
                enable: [],
                disable: [],
            },
            overrides: [],
        },
        server: {
            port,
            url: `http://localhost:${port}`,
            redirect: 'https://git.trss.me/Yunzai',
            access_token: '',
            extra_auth_headers: [],
        },
        redis: {
            path: 'redis-server',
            host: '127.0.0.1',
            port: port + 1,
            username: '',
            password: '',
            db: 0,
        },
        renderer: { name: '' },
    };
}

export function newYunzaiGroupOverride(key = ''): YunzaiGroupOverride {
    return { key };
}

const LOOPBACK = new Set(['127.0.0.1', 'localhost', '::1']);

function checkIds(out: AppConfigIssue[], path: string, items: readonly string[]) {
    items.forEach((v, i) => {
        const t = v.trim();
        if (!t) out.push({ path: `${path}/${i}`, message: '不能留空' });
        else if (/\s/.test(t)) out.push({ path: `${path}/${i}`, message: '不能带空格' });
    });
}

/** 规则与后端 `config::validate` 对齐，path 也一样 */
export function validateYunzaiConfig(cfg: YunzaiInstanceConfig): AppConfigIssue[] {
    const out: AppConfigIssue[] = [];
    if (!(YUNZAI_LOG_LEVELS as readonly string[]).includes(cfg.bot.log_level)) {
        out.push({ path: 'bot/log_level', message: `只能是 ${YUNZAI_LOG_LEVELS.join(' / ')} 之一` });
    }
    if (!YUNZAI_RENDERERS.some((r) => r.value === cfg.renderer.name)) {
        out.push({ path: 'renderer/name', message: '只能是自动、puppeteer 或 shotium' });
    }
    if (!Number.isInteger(cfg.server.port) || cfg.server.port < 1 || cfg.server.port > 65535) {
        out.push({ path: 'server/port', message: '端口要在 1 到 65535 之间' });
    }
    if (!Number.isInteger(cfg.redis.port) || cfg.redis.port < 1 || cfg.redis.port > 65535) {
        out.push({ path: 'redis/port', message: '端口要在 1 到 65535 之间' });
    } else if (cfg.redis.port === cfg.server.port && LOOPBACK.has(cfg.redis.host.trim())) {
        out.push({ path: 'redis/port', message: '不能和云崽自己的端口相同' });
    }
    if (!cfg.redis.host.trim()) out.push({ path: 'redis/host', message: '不能留空' });
    if (!cfg.redis.path.trim()) out.push({ path: 'redis/path', message: '不能留空' });
    if (/\s/.test(cfg.server.access_token)) {
        out.push({ path: 'server/access_token', message: '不能带空格' });
    }
    checkIds(out, 'other/master_qq', cfg.other.master_qq);
    cfg.other.master.forEach((pair, i) => {
        const [bot, ...rest] = pair.split(':');
        if (!bot?.trim() || !rest.join(':').trim()) {
            out.push({ path: `other/master/${i}`, message: '写成 Bot号:主人号' });
        }
    });
    checkIds(out, 'other/white_group', cfg.other.white_group);
    checkIds(out, 'other/white_user', cfg.other.white_user);
    checkIds(out, 'other/black_group', cfg.other.black_group);
    checkIds(out, 'other/black_user', cfg.other.black_user);
    if (cfg.group.default.only_reply_at > 2) {
        out.push({ path: 'group/default/only_reply_at', message: '只能是 0、1、2' });
    }
    if (cfg.group.default.add_limit > 2) {
        out.push({ path: 'group/default/add_limit', message: '只能是 0、1、2' });
    }
    const seen = new Set<string>();
    cfg.group.overrides.forEach((o, i) => {
        const key = o.key.trim();
        const path = `group/overrides/${i}/key`;
        if (!key) out.push({ path, message: '填群号、Bot号:default 或 Bot号:群号' });
        else if (key === 'default') out.push({ path, message: 'default 是所有群的默认，不用再单独加' });
        else if (/[\s#]/.test(key)) out.push({ path, message: '不能带空格或 #' });
        else if (seen.has(key)) out.push({ path, message: '重复了' });
        seen.add(key);
    });
    return out;
}

export const YUNZAI_CONFIG_FORM: ConfigFormSpec<'yunzai'> = {
    framework: 'yunzai',
    validate: validateYunzaiConfig,
    saveHint: (r, running) => {
        if (r.restart_required) return '端口、Redis、定时任务这些改动重启后生效';
        return running ? '云崽几秒内会自己读到新配置' : null;
    },
};

export function yunzaiLinkInputsChanged(a: YunzaiInstanceConfig, b: YunzaiInstanceConfig): boolean {
    return a.server.port !== b.server.port || a.server.access_token !== b.server.access_token;
}

/** 和后端 `restart_inputs_changed` 同一份清单：只在启动时读的那些 */
export function yunzaiRestartInputsChanged(a: YunzaiInstanceConfig, b: YunzaiInstanceConfig): boolean {
    const same = (x: unknown, y: unknown) => JSON.stringify(x) === JSON.stringify(y);
    return (
        a.server.port !== b.server.port
        || !same(a.redis, b.redis)
        || a.bot.file_watch !== b.bot.file_watch
        || a.bot.update_time !== b.bot.update_time
        || a.bot.restart_time !== b.bot.restart_time
        || !same(a.bot.update_cron, b.bot.update_cron)
        || !same(a.bot.restart_cron, b.bot.restart_cron)
        || !same(a.bot.stop_cron, b.bot.stop_cron)
        || !same(a.bot.start_cron, b.bot.start_cron)
        || a.bot.plugin_load_timeout !== b.bot.plugin_load_timeout
    );
}

/** 没设主人：群里 #设置主人 也行，但桌面端直接填更省事 */
export function yunzaiNeedsMaster(cfg: YunzaiInstanceConfig): boolean {
    return cfg.other.master_qq.length === 0 && cfg.other.master.length === 0;
}

/** 一条单独设置里写了几项（卡片标题旁的计数） */
export function yunzaiOverrideFieldCount(o: YunzaiGroupOverride): number {
    return Object.entries(o).filter(([k, v]) => k !== 'key' && v !== undefined && v !== null).length;
}

/** 单独设置的键是什么意思，给卡片副标题 */
export function yunzaiOverrideScope(key: string): string {
    const k = key.trim();
    if (!k) return '还没填';
    if (/^[^:]+:default$/.test(k)) return `Bot ${k.split(':')[0]} 在所有群`;
    if (k.includes(':')) {
        const [bot, group] = k.split(':');
        return `Bot ${bot} 在群 ${group}`;
    }
    return `群 ${k}`;
}
