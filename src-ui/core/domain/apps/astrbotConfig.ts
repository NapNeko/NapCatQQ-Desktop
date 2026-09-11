// AstrBot 窄配置的前端校验 / 默认值。规则与后端 `AstrBotInstanceConfig::validate` 对齐。

import type {
    AppConfigIssue,
    AstrBotAiSettings,
    AstrBotInstanceConfig,
    AstrBotKbBind,
    AstrBotPlatformGates,
    AstrBotProviderModel,
    AstrBotProviderSource,
    AstrBotSttSettings,
    AstrBotSubagentConfig,
    AstrBotTtsSettings,
    AstrBotWebSearchSettings,
} from '../../ipc/types';

export const ASTRBOT_PROVIDER_TYPES = [
    'chat_completion',
    'speech_to_text',
    'text_to_speech',
    'embedding',
    'rerank',
    'agent_runner',
] as const;

export function isKnownProviderType(t: string): boolean {
    return (ASTRBOT_PROVIDER_TYPES as readonly string[]).includes(t);
}

export function isChatProviderType(t: string): boolean {
    return t === 'chat_completion' || t === '';
}

export function isEmbeddingProviderType(t: string): boolean {
    return t === 'embedding';
}

export function astrbotDefaultAi(): AstrBotAiSettings {
    return {
        enable: true,
        default_provider_id: '',
        fallback_chat_models: [],
        default_image_caption_provider_id: '',
        image_caption_prompt: 'Please describe the image using Chinese.',
        default_personality: 'default',
        wake_prefix: '',
        prompt_prefix: '{{prompt}}',
        agent_runner_type: 'local',
        dify_agent_runner_provider_id: '',
        coze_agent_runner_provider_id: '',
        dashscope_agent_runner_provider_id: '',
        deerflow_agent_runner_provider_id: '',
        max_context_length: -1,
        dequeue_context_length: 1,
        context_limit_reached_strategy: 'truncate_by_turns',
        llm_compress_instruction: '',
        llm_compress_keep_recent: 6,
        llm_compress_provider_id: '',
        streaming_response: false,
        display_reasoning_text: false,
        llm_safety_mode: true,
        identifier: false,
        group_name_display: false,
        datetime_system_prompt: true,
        show_tool_use_status: false,
        show_tool_call_result: false,
        max_agent_step: 30,
        tool_call_timeout: 60,
        tool_schema_mode: 'full',
    };
}

export function astrbotDefaultStt(): AstrBotSttSettings {
    return { enable: false, provider_id: '' };
}

export function astrbotDefaultTts(): AstrBotTtsSettings {
    return { enable: false, provider_id: '', dual_output: false, trigger_probability: 1 };
}

export function astrbotDefaultWebSearch(): AstrBotWebSearchSettings {
    return {
        enable: false,
        provider: 'default',
        tavily_key: [],
        bocha_key: [],
        baidu_app_builder_key: '',
        show_link: false,
    };
}

export function astrbotDefaultKb(): AstrBotKbBind {
    return { names: [], fusion_top_k: 20, final_top_k: 5, agentic_mode: false };
}

export function astrbotDefaultGates(): AstrBotPlatformGates {
    return {
        wake_prefix: ['/'],
        unique_session: false,
        friend_message_needs_wake_prefix: false,
        enable_id_white_list: true,
        id_whitelist: [],
        id_whitelist_log: true,
        wl_ignore_admin_on_group: true,
        wl_ignore_admin_on_friend: true,
        admins_id: ['astrbot'],
    };
}

export function astrbotDefaultSubagent(): AstrBotSubagentConfig {
    return {
        main_enable: false,
        remove_main_duplicate_tools: false,
        router_system_prompt: '',
        agents: [],
    };
}

export function newOpenAiSource(id = 'openai'): AstrBotProviderSource {
    return {
        id,
        provider: 'openai',
        type: 'openai_chat_completion',
        provider_type: 'chat_completion',
        enable: true,
        key: [],
        api_base: 'https://api.openai.com/v1',
        timeout: 120,
        proxy: '',
    };
}

export type AstrBotSourcePreset = {
    id: string;
    label: string;
    provider: string;
    type: string;
    provider_type: string;
    api_base: string;
    /** 本机服务（Ollama / LM Studio）不需要 key，也不该催用户填 */
    local?: boolean;
};

// id / type / provider / api_base 抄自 AstrBot `astrbot/core/config/default.py` 的模板，别凭记忆改。
export const ASTRBOT_SOURCE_PRESETS: readonly AstrBotSourcePreset[] = [
    { id: 'openai', label: 'OpenAI', provider: 'openai', type: 'openai_chat_completion', provider_type: 'chat_completion', api_base: 'https://api.openai.com/v1' },
    { id: 'deepseek', label: 'DeepSeek', provider: 'deepseek', type: 'openai_chat_completion', provider_type: 'chat_completion', api_base: 'https://api.deepseek.com/v1' },
    { id: 'moonshot', label: 'Moonshot / Kimi', provider: 'moonshot', type: 'openai_chat_completion', provider_type: 'chat_completion', api_base: 'https://api.moonshot.cn/v1' },
    { id: 'zhipu', label: '智谱 GLM', provider: 'zhipu', type: 'zhipu_chat_completion', provider_type: 'chat_completion', api_base: 'https://open.bigmodel.cn/api/paas/v4/' },
    { id: 'siliconflow', label: '硅基流动', provider: 'siliconflow', type: 'openai_chat_completion', provider_type: 'chat_completion', api_base: 'https://api.siliconflow.cn/v1' },
    { id: 'google_gemini_openai', label: 'Gemini（OpenAI 兼容）', provider: 'google', type: 'openai_chat_completion', provider_type: 'chat_completion', api_base: 'https://generativelanguage.googleapis.com/v1beta/openai/' },
    { id: 'anthropic', label: 'Anthropic', provider: 'anthropic', type: 'anthropic_chat_completion', provider_type: 'chat_completion', api_base: 'https://api.anthropic.com' },
    { id: 'openrouter', label: 'OpenRouter', provider: 'openrouter', type: 'openrouter_chat_completion', provider_type: 'chat_completion', api_base: 'https://openrouter.ai/api/v1' },
    { id: 'ollama', label: 'Ollama（本机）', provider: 'ollama', type: 'openai_chat_completion', provider_type: 'chat_completion', api_base: 'http://127.0.0.1:11434/v1', local: true },
    { id: 'lm_studio', label: 'LM Studio（本机）', provider: 'lm_studio', type: 'openai_chat_completion', provider_type: 'chat_completion', api_base: 'http://127.0.0.1:1234/v1', local: true },
    { id: 'custom', label: '自定义 OpenAI 兼容接口', provider: 'openai', type: 'openai_chat_completion', provider_type: 'chat_completion', api_base: '' },
    { id: 'openai_embedding', label: 'OpenAI Embedding', provider: 'openai', type: 'openai_embedding', provider_type: 'embedding', api_base: 'https://api.openai.com/v1' },
    { id: 'ollama_embedding', label: 'Ollama Embedding（本机）', provider: 'ollama', type: 'ollama_embedding', provider_type: 'embedding', api_base: 'http://127.0.0.1:11434', local: true },
];

/** 在已有 id 集合里挑一个不撞的：`deepseek` → `deepseek-2` → `deepseek-3` */
export function uniqueId(base: string, taken: Iterable<string>): string {
    const set = new Set(taken);
    if (!set.has(base)) return base;
    let n = 2;
    while (set.has(`${base}-${n}`)) n += 1;
    return `${base}-${n}`;
}

export function newSourceFromPreset(
    preset: AstrBotSourcePreset,
    existing: readonly AstrBotProviderSource[],
): AstrBotProviderSource {
    return {
        id: uniqueId(preset.id, existing.map((s) => s.id)),
        provider: preset.provider,
        type: preset.type,
        provider_type: preset.provider_type,
        enable: true,
        key: [],
        api_base: preset.api_base,
        timeout: 120,
        proxy: '',
    };
}

export function presetLabel(src: AstrBotProviderSource): string | undefined {
    return ASTRBOT_SOURCE_PRESETS.find(
        (p) => p.type === src.type && p.provider === src.provider && p.provider_type === src.provider_type,
    )?.label;
}

export function newOpenAiModel(sourceId: string, model = 'gpt-4o-mini'): AstrBotProviderModel {
    return {
        id: `${sourceId}/${model}`,
        enable: true,
        provider_source_id: sourceId,
        model,
        modalities: ['text'],
        max_context_tokens: 0,
    };
}

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
        sources: [],
        models: [],
        ai: astrbotDefaultAi(),
        stt: astrbotDefaultStt(),
        tts: astrbotDefaultTts(),
        websearch: astrbotDefaultWebSearch(),
        kb: astrbotDefaultKb(),
        gates: astrbotDefaultGates(),
        subagent: astrbotDefaultSubagent(),
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
    const seenSrc = new Set<string>();
    cfg.sources.forEach((s, i) => {
        if (!s.id.trim()) {
            out.push({ path: `sources/${i}/id`, message: '提供商 id 不能为空' });
            return;
        }
        if (seenSrc.has(s.id)) out.push({ path: `sources/${i}/id`, message: '提供商 id 重复' });
        seenSrc.add(s.id);
    });
    const sourceIds = new Set(cfg.sources.map((s) => s.id));
    const seenModel = new Set<string>();
    cfg.models.forEach((m, i) => {
        if (!m.id.trim()) {
            out.push({ path: `models/${i}/id`, message: '模型 id 不能为空' });
        } else if (seenModel.has(m.id)) {
            out.push({ path: `models/${i}/id`, message: '模型 id 重复' });
        } else {
            seenModel.add(m.id);
        }
        if (m.provider_source_id && !sourceIds.has(m.provider_source_id)) {
            out.push({ path: `models/${i}/provider_source_id`, message: '模型引用了不存在的提供商' });
        }
    });
    return out;
}

export function astrbotLinkInputsChanged(a: AstrBotInstanceConfig, b: AstrBotInstanceConfig): boolean {
    return (
        a.onebot.ws_reverse_port !== b.onebot.ws_reverse_port ||
        a.onebot.ws_reverse_token !== b.onebot.ws_reverse_token
    );
}

export function enabledChatModels(cfg: AstrBotInstanceConfig): AstrBotProviderModel[] {
    const sources = new Map(cfg.sources.map((s) => [s.id, s]));
    return cfg.models.filter((m) => {
        const src = sources.get(m.provider_source_id);
        return m.enable && !!src?.enable && isChatProviderType(src.provider_type);
    });
}

export function embeddingSources(cfg: AstrBotInstanceConfig): AstrBotProviderSource[] {
    return cfg.sources.filter((s) => s.enable && isEmbeddingProviderType(s.provider_type));
}

/** 某一用途（speech_to_text / text_to_speech …）下所有启用的模型 */
export function modelsOfType(cfg: AstrBotInstanceConfig, providerType: string): AstrBotProviderModel[] {
    const sources = new Map(cfg.sources.map((s) => [s.id, s]));
    return cfg.models.filter((m) => {
        const src = sources.get(m.provider_source_id);
        return m.enable && !!src?.enable && src.provider_type === providerType;
    });
}

/** 只支持 6 个上游认的会话规则键；别的键 update-rule 会直接拒 */
export const ASTRBOT_SESSION_RULE_KEYS = [
    'session_service_config',
    'provider_perf_chat_completion',
    'provider_perf_speech_to_text',
    'provider_perf_text_to_speech',
    'kb_config',
    'session_plugin_config',
] as const;

export type AstrBotSessionRuleKey = (typeof ASTRBOT_SESSION_RULE_KEYS)[number];

export function isKnownSessionRuleKey(k: string): k is AstrBotSessionRuleKey {
    return (ASTRBOT_SESSION_RULE_KEYS as readonly string[]).includes(k);
}

export type AstrBotSessionServiceConfig = {
    session_enabled: boolean;
    llm_enabled: boolean;
    tts_enabled: boolean;
    custom_name: string;
};

/** 上游缺字段就当 true（和 list-all-with-status 的取值逻辑一致） */
export function parseSessionServiceConfig(json: string): AstrBotSessionServiceConfig {
    let raw: unknown = null;
    try {
        raw = JSON.parse(json);
    } catch {
        raw = null;
    }
    const o = raw && typeof raw === 'object' ? (raw as Record<string, unknown>) : {};
    const flag = (k: string) => (typeof o[k] === 'boolean' ? (o[k] as boolean) : true);
    return {
        session_enabled: flag('session_enabled'),
        llm_enabled: flag('llm_enabled'),
        tts_enabled: flag('tts_enabled'),
        custom_name: typeof o.custom_name === 'string' ? o.custom_name : '',
    };
}

export function parseJsonString(json: string): string {
    try {
        const v = JSON.parse(json);
        return typeof v === 'string' ? v : '';
    } catch {
        return '';
    }
}

export type AstrBotReadiness = {
    hasSource: boolean;
    hasEnabledChat: boolean;
    defaultOk: boolean;
    llmOn: boolean;
    whitelistMayBlock: boolean;
};

export function astrbotReadiness(cfg: AstrBotInstanceConfig): AstrBotReadiness {
    const chat = enabledChatModels(cfg);
    return {
        hasSource: cfg.sources.length > 0,
        hasEnabledChat: chat.length > 0,
        defaultOk: !!cfg.ai.default_provider_id && chat.some((m) => m.id === cfg.ai.default_provider_id),
        llmOn: cfg.ai.enable,
        whitelistMayBlock: cfg.gates.enable_id_white_list && cfg.gates.id_whitelist.length === 0,
    };
}
