// MaiBot 类型化配置的前端校验 / 上手判定。规则与后端 `maibot::config::validate` 对齐，
// 路径前缀同后端：`bot/…` 是 bot_config，`models/…` 是 model_config，`adapter/…` 是适配器名单。

import type { ConfigFormSpec } from './appConfigForm';
import type {
    AppConfigIssue,
    MaiBotAPIProvider,
    MaiBotBotConfigFile,
    MaiBotChatFilter,
    MaiBotInstanceConfig,
    MaiBotKeywordRuleConfig,
    MaiBotModelConfigFile,
} from '../../ipc/types';
// 由 Rust 的 export_bindings_maibot_defaults 导出：和 IPC 上的序列化逐字一致
import maibotDefaults from './maibotDefaults.json';
import { BOT_SCHEMA, MODEL_SCHEMA, newItemFor, nodeAt, schemaIssues } from './maibotSchema';

type MaiBotFiles = Pick<MaiBotInstanceConfig, 'bot' | 'models'>;

export function maibotDefaultChat(): MaiBotChatFilter {
    return {
        enable_chat_list_filter: true,
        group_list_type: 'whitelist',
        group_list: [],
        private_list_type: 'whitelist',
        private_list: [],
        ban_user_id: [],
    };
}

/** 上游默认的两份主配置（model_config 是首启写出来的那份，带一组 DeepSeek） */
export function maibotDefaultFiles(): MaiBotFiles {
    // JSON 推不出字面量联合类型，这里按生成的 TS 类型认
    return structuredClone(maibotDefaults) as unknown as MaiBotFiles;
}

export function maibotDefaultConfig(webuiPort: number): MaiBotInstanceConfig {
    const files = maibotDefaultFiles();
    files.bot.webui.port = webuiPort;
    files.bot.maim_message.ws_server_port = webuiPort + 1;
    return {
        ...files,
        webui_token: '',
        adapter: {
            enabled: false,
            napcat_host: '127.0.0.1',
            napcat_port: 3001,
            has_token: false,
            chat: maibotDefaultChat(),
        },
    };
}

/** 上游默认就是这样：白名单开着、群聊私聊名单都空，适配器把所有消息丢掉，麦麦一句都不回 */
export function maibotChatDropsEverything(chat: MaiBotChatFilter): boolean {
    return (
        chat.enable_chat_list_filter &&
        chat.group_list_type === 'whitelist' &&
        chat.group_list.length === 0 &&
        chat.private_list_type === 'whitelist' &&
        chat.private_list.length === 0
    );
}

/** 概览里「回复范围」那一格的说法 */
export function maibotChatScope(chat: MaiBotChatFilter): string {
    if (!chat.enable_chat_list_filter) return '所有群聊和私聊';
    const side = (mode: MaiBotChatFilter['group_list_type'], list: string[], what: string) => {
        if (mode === 'whitelist') return list.length ? `${list.length} 个${what}` : `不回${what}`;
        return list.length ? `${what}（除 ${list.length} 个）` : `所有${what}`;
    };
    return `${side(chat.group_list_type, chat.group_list, '群')}，${side(chat.private_list_type, chat.private_list, '私聊')}`;
}

const DIGITS = /^\d+$/;

const validPort = (p: number) => Number.isInteger(p) && p >= 1 && p <= 65535;

/** 麦麦开箱那份 model_config 里提供商的 API Key 占位，没换掉就等于没配模型 */
export const MAIBOT_PLACEHOLDER_API_KEY = 'your-api-key';

export const MAIBOT_TASK_KEYS = [
    'replyer',
    'planner',
    'memory',
    'mid_memory',
    'utils',
    'learner',
    'expression_use',
    'emoji',
    'vlm',
    'voice',
    'embedding',
] as const satisfies readonly (keyof MaiBotModelConfigFile['model_task_config'])[];

export type MaiBotModelSetupIssue = 'no_provider' | 'placeholder_key' | 'no_task_model' | null;

/** 上游说这三个任务是必须的：回复、规划、杂务（概括整理这些小活） */
export const MAIBOT_REQUIRED_TASKS = ['replyer', 'planner', 'utils'] as const;

/**
 * 模型能不能用：没有提供商、必需任务用到的提供商 Key 还是占位、必需任务没挑模型，麦麦都说不了话。
 * 概览清单、侧栏圆点、模型页顶的提示都用这一个判定。
 */
export function maibotModelSetupIssue(models: MaiBotModelConfigFile): MaiBotModelSetupIssue {
    if (!models.api_providers.length) return 'no_provider';
    const t = models.model_task_config;
    const picked = new Set(MAIBOT_REQUIRED_TASKS.flatMap((k) => t[k].model_list));
    const used = new Set(
        models.models.filter((m) => picked.has(m.name)).map((m) => m.api_provider),
    );
    const placeholder = models.api_providers.some(
        (p) =>
            used.has(p.name) &&
            p.auth_type !== 'none' &&
            p.api_key.trim() === MAIBOT_PLACEHOLDER_API_KEY,
    );
    if (placeholder) return 'placeholder_key';
    if (MAIBOT_REQUIRED_TASKS.some((k) => !t[k].model_list.length)) return 'no_task_model';
    return null;
}

export type MaiBotProviderPreset = {
    id: string;
    label: string;
    /** 写进配置的提供商名，模型按它引用 */
    name: string;
    base_url: string;
    client_type: 'openai' | 'openai_responses' | 'gemini';
    /** 本机跑的服务，不鉴权 */
    local?: boolean;
};

// 照上游 WebUI 的 providerTemplates，去掉接口不兼容的几家，补上两个本机服务
export const MAIBOT_PROVIDER_PRESETS: readonly MaiBotProviderPreset[] = [
    {
        id: 'deepseek',
        label: 'DeepSeek',
        name: 'DeepSeek',
        base_url: 'https://api.deepseek.com',
        client_type: 'openai',
    },
    {
        id: 'siliconflow',
        label: '硅基流动',
        name: 'SiliconFlow',
        base_url: 'https://api.siliconflow.cn/v1',
        client_type: 'openai',
    },
    {
        id: 'alibaba',
        label: '阿里云百炼',
        name: 'Alibaba',
        base_url: 'https://dashscope.aliyuncs.com/compatible-mode/v1',
        client_type: 'openai',
    },
    {
        id: 'zhipu',
        label: '智谱 GLM',
        name: 'ZhipuAI',
        base_url: 'https://open.bigmodel.cn/api/paas/v4',
        client_type: 'openai',
    },
    {
        id: 'moonshot',
        label: 'Moonshot / Kimi',
        name: 'Moonshot',
        base_url: 'https://api.moonshot.cn/v1',
        client_type: 'openai',
    },
    {
        id: 'doubao',
        label: '火山方舟（豆包）',
        name: 'Doubao',
        base_url: 'https://ark.cn-beijing.volces.com/api/v3',
        client_type: 'openai',
    },
    {
        id: 'minimax',
        label: 'MiniMax',
        name: 'MiniMax',
        base_url: 'https://api.minimax.chat/v1',
        client_type: 'openai',
    },
    {
        id: 'stepfun',
        label: '阶跃星辰',
        name: 'StepFun',
        base_url: 'https://api.stepfun.com/v1',
        client_type: 'openai',
    },
    {
        id: 'openai',
        label: 'OpenAI',
        name: 'OpenAI',
        base_url: 'https://api.openai.com/v1',
        client_type: 'openai',
    },
    {
        id: 'gemini',
        label: 'Google Gemini',
        name: 'Gemini',
        base_url: 'https://generativelanguage.googleapis.com/v1beta',
        client_type: 'gemini',
    },
    {
        id: 'openrouter',
        label: 'OpenRouter',
        name: 'OpenRouter',
        base_url: 'https://openrouter.ai/api/v1',
        client_type: 'openai',
    },
    {
        id: 'xai',
        label: 'xAI（Grok）',
        name: 'xAI',
        base_url: 'https://api.x.ai/v1',
        client_type: 'openai',
    },
    {
        id: 'groq',
        label: 'Groq',
        name: 'Groq',
        base_url: 'https://api.groq.com/openai/v1',
        client_type: 'openai',
    },
    {
        id: 'mistral',
        label: 'Mistral',
        name: 'Mistral',
        base_url: 'https://api.mistral.ai/v1',
        client_type: 'openai',
    },
    {
        id: 'ollama',
        label: 'Ollama（本机）',
        name: 'Ollama',
        base_url: 'http://127.0.0.1:11434/v1',
        client_type: 'openai',
        local: true,
    },
    {
        id: 'lm_studio',
        label: 'LM Studio（本机）',
        name: 'LMStudio',
        base_url: 'http://127.0.0.1:1234/v1',
        client_type: 'openai',
        local: true,
    },
    {
        id: 'custom',
        label: '自定义 OpenAI 兼容接口',
        name: '',
        base_url: '',
        client_type: 'openai',
    },
];

/** 照预设起一个新提供商：其余字段取上游默认；名字撞了往后编号，自定义的给个占位名 */
export function newMaiBotProvider(
    existing: readonly MaiBotAPIProvider[],
    preset: MaiBotProviderPreset,
): MaiBotAPIProvider {
    const node = nodeAt(MODEL_SCHEMA, ['api_providers']);
    const base = (node ? newItemFor(node) : {}) as MaiBotAPIProvider;
    const stem = preset.name || '提供商';
    const taken = new Set(existing.map((p) => p.name));
    let name = stem;
    for (let n = 2; taken.has(name); n += 1) name = `${stem} ${n}`;
    return {
        ...base,
        name,
        base_url: preset.base_url,
        client_type: preset.client_type,
        api_key: '',
        auth_type: preset.local ? 'none' : base.auth_type,
    };
}

/**
 * 提供商改名后（列表里已经是新名字），把引用旧名字的模型跟过去。
 * 还有别的提供商叫旧名字时不动，免得把人家的模型也挪过来。
 */
export function renameMaiBotProvider(
    models: MaiBotModelConfigFile,
    from: string,
    to: string,
): MaiBotModelConfigFile {
    if (from === to || !from) return models;
    if (models.api_providers.some((p) => p.name === from)) return models;
    return {
        ...models,
        models: models.models.map((m) =>
            m.api_provider === from ? { ...m, api_provider: to } : m,
        ),
    };
}

/** 模型改名后，任务里挑了旧名字的跟过去；规则同上 */
export function renameMaiBotModel(
    models: MaiBotModelConfigFile,
    from: string,
    to: string,
): MaiBotModelConfigFile {
    if (from === to || !from) return models;
    if (models.models.some((m) => m.name === from)) return models;
    const tasks = { ...models.model_task_config };
    for (const key of MAIBOT_TASK_KEYS) {
        const task = tasks[key];
        if (task.model_list.includes(from)) {
            tasks[key] = { ...task, model_list: task.model_list.map((n) => (n === from ? to : n)) };
        }
    }
    return { ...models, model_task_config: tasks };
}

type Issues = AppConfigIssue[];
const blank = (s: string | null | undefined) => !s || !s.trim();

function botRules(bot: MaiBotBotConfigFile, out: Issues) {
    const kw = bot.keyword_reaction;
    const rule = (path: string, r: MaiBotKeywordRuleConfig) => {
        if (!r.keywords.length && !r.regex.length)
            out.push({ path: `${path}/keywords`, message: '关键词和正则至少填一个' });
        if (blank(r.reaction))
            out.push({ path: `${path}/reaction`, message: '要写命中后给麦麦的提示' });
    };
    kw.keyword_rules.forEach((r, i) => rule(`bot/keyword_reaction/keyword_rules/${i}`, r));
    kw.regex_rules.forEach((r, i) => rule(`bot/keyword_reaction/regex_rules/${i}`, r));

    bot.chat.reply_style.chat_prompts.forEach((item, i) => {
        const filled = [item.platform, item.item_id, item.prompt].map((s) => !blank(s));
        if (filled.some(Boolean) && !filled.every(Boolean)) {
            (['platform', 'item_id', 'prompt'] as const).forEach((name, j) => {
                if (!filled[j]) {
                    out.push({
                        path: `bot/chat/reply_style/chat_prompts/${i}/${name}`,
                        message: '平台、聊天 ID 和提示词要一起填',
                    });
                }
            });
        }
    });

    const mem = bot.a_memorix;
    if (mem.threshold.min_threshold >= mem.threshold.max_threshold) {
        out.push({ path: 'bot/a_memorix/threshold/min_threshold', message: '要小于最大阈值' });
    }
    if (mem.memory.revive_threshold <= mem.memory.prune_threshold) {
        out.push({ path: 'bot/a_memorix/memory/revive_threshold', message: '要大于修剪阈值' });
    }

    const mcp = bot.mcp;
    mcp.client.roots.items.forEach((root, i) => {
        if (root.enabled && blank(root.uri))
            out.push({
                path: `bot/mcp/client/roots/items/${i}/uri`,
                message: '启用的目录要填 uri',
            });
    });
    const eli = mcp.client.elicitation;
    if (eli.enable && !(eli.allow_form || eli.allow_url)) {
        out.push({
            path: 'bot/mcp/client/elicitation/allow_form',
            message: '开启后至少允许一种方式',
        });
    }
    const names = new Set<string>();
    mcp.servers.forEach((s, i) => {
        if (!s.enabled) return;
        const path = `bot/mcp/servers/${i}`;
        const name = s.name.trim();
        if (!name) out.push({ path: `${path}/name`, message: '要起个名字' });
        else if (names.has(name))
            out.push({ path: `${path}/name`, message: '和别的 MCP 服务重名了' });
        names.add(name);
        if (s.transport === 'stdio' && blank(s.command))
            out.push({ path: `${path}/command`, message: 'stdio 方式要填启动命令' });
        if ((s.transport === 'streamable_http' || s.transport === 'sse') && blank(s.url)) {
            out.push({ path: `${path}/url`, message: '这种连接方式要填地址' });
        }
        if (s.authorization.mode === 'bearer' && blank(s.authorization.bearer_token)) {
            out.push({
                path: `${path}/authorization/bearer_token`,
                message: 'bearer 认证要填 token',
            });
        }
    });
}

function modelRules(models: MaiBotModelConfigFile, out: Issues) {
    if (!models.api_providers.length)
        out.push({ path: 'models/api_providers', message: '至少要有一个提供商' });
    if (!models.models.length) out.push({ path: 'models/models', message: '至少要有一个模型' });
    const providers = new Set<string>();
    models.api_providers.forEach((p, i) => {
        const path = `models/api_providers/${i}`;
        if (blank(p.name)) out.push({ path: `${path}/name`, message: '要起个名字' });
        else if (providers.has(p.name))
            out.push({ path: `${path}/name`, message: '和别的提供商重名了' });
        providers.add(p.name);
        if (p.auth_type !== 'none' && blank(p.api_key))
            out.push({ path: `${path}/api_key`, message: '要填 API Key' });
        if (p.client_type !== 'gemini' && blank(p.base_url))
            out.push({ path: `${path}/base_url`, message: '要填接口地址' });
        if (p.auth_type === 'header' && blank(p.auth_header_name)) {
            out.push({ path: `${path}/auth_header_name`, message: '请求头认证要填头名' });
        }
        if (p.auth_type === 'query' && blank(p.auth_query_name)) {
            out.push({ path: `${path}/auth_query_name`, message: '查询参数认证要填参数名' });
        }
    });
    const names = new Set<string>();
    models.models.forEach((m, i) => {
        const path = `models/models/${i}`;
        if (blank(m.name)) out.push({ path: `${path}/name`, message: '要起个名字' });
        else if (names.has(m.name)) out.push({ path: `${path}/name`, message: '和别的模型重名了' });
        names.add(m.name);
        if (blank(m.model_identifier))
            out.push({ path: `${path}/model_identifier`, message: '要填服务商那边的模型标识' });
        if (!providers.has(m.api_provider))
            out.push({ path: `${path}/api_provider`, message: '选一个已有的提供商' });
    });
    for (const task of MAIBOT_TASK_KEYS) {
        models.model_task_config[task].model_list.forEach((name, j) => {
            if (!names.has(name)) {
                out.push({
                    path: `models/model_task_config/${task}/model_list/${j}`,
                    message: `没有叫 ${JSON.stringify(name)} 的模型`,
                });
            }
        });
    }
}

export function validateMaiBotConfig(cfg: MaiBotInstanceConfig): AppConfigIssue[] {
    const out: AppConfigIssue[] = [];
    const webui = cfg.bot.webui.port;
    const legacy = cfg.bot.maim_message.ws_server_port;
    if (!validPort(webui)) out.push({ path: 'bot/webui/port', message: '要在 1 到 65535 之间' });
    if (!validPort(legacy)) {
        out.push({ path: 'bot/maim_message/ws_server_port', message: '要在 1 到 65535 之间' });
    } else if (webui === legacy) {
        out.push({ path: 'bot/maim_message/ws_server_port', message: '不能和 WebUI 用同一个端口' });
    }
    const mm = cfg.bot.maim_message;
    if (mm.enable_api_server) {
        if (!validPort(mm.api_server_port)) {
            out.push({ path: 'bot/maim_message/api_server_port', message: '要在 1 到 65535 之间' });
        } else if (mm.api_server_port === webui || mm.api_server_port === legacy) {
            out.push({
                path: 'bot/maim_message/api_server_port',
                message: '不能和 WebUI 或旧版消息服务用同一个端口',
            });
        }
    }
    out.push(...schemaIssues(BOT_SCHEMA, cfg.bot, 'bot'));
    botRules(cfg.bot, out);
    out.push(...schemaIssues(MODEL_SCHEMA, cfg.models, 'models'));
    modelRules(cfg.models, out);
    const chat = cfg.adapter?.chat;
    if (chat) {
        const lists: [string, string[], string][] = [
            ['adapter/chat/group_list', chat.group_list, '群号'],
            ['adapter/chat/private_list', chat.private_list, 'QQ 号'],
            ['adapter/chat/ban_user_id', chat.ban_user_id, 'QQ 号'],
        ];
        for (const [path, list, what] of lists) {
            const bad = list.find((s) => !DIGITS.test(s.trim()));
            if (bad !== undefined) out.push({ path, message: `${what}只能是数字：${bad}` });
        }
    }
    // 范围检查和手写规则有重叠，同一栏只报第一条（后端同样处理）
    const seen = new Set<string>();
    return out.filter((i) => !seen.has(i.path) && !!seen.add(i.path));
}

export const MAIBOT_CONFIG_FORM: ConfigFormSpec<'maibot'> = {
    framework: 'maibot',
    validate: validateMaiBotConfig,
    // 麦麦和适配器都热加载配置；只有端口、日志、插件运行时这些启动时读的要重启
    saveHint: (r) => (r.restart_required ? '端口、日志这类启动时读的设置要重启麦麦才生效' : null),
};
