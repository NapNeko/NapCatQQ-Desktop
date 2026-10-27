// 内置模型配置的表单校验，不发起网络请求。

import type { AssistantApiFormat, AssistantBuiltinSettings, AssistantModelPreset } from './types';

export const assistantApiFormats: { value: AssistantApiFormat; label: string }[] = [
    { value: 'openAiChatCompletions', label: 'OpenAI Chat Completions' },
    { value: 'openAiResponses', label: 'OpenAI Responses' },
    { value: 'anthropicMessages', label: 'Anthropic Messages' },
    { value: 'geminiGenerateContent', label: 'Gemini GenerateContent' },
];

interface ModelPreset {
    value: AssistantModelPreset;
    label: string;
    baseUrl: string;
    apiFormat: AssistantApiFormat;
    models: string[];
}

export const assistantModelPresets: ModelPreset[] = [
    {
        value: 'custom',
        label: '自定义',
        baseUrl: '',
        apiFormat: 'openAiChatCompletions',
        models: [],
    },
    {
        value: 'openAi',
        label: 'OpenAI',
        baseUrl: 'https://api.openai.com/v1',
        apiFormat: 'openAiResponses',
        models: ['gpt-6.1-sol'],
    },
    {
        value: 'anthropic',
        label: 'Anthropic',
        baseUrl: 'https://api.anthropic.com/v1',
        apiFormat: 'anthropicMessages',
        models: ['claude-sonnet-5-5'],
    },
    {
        value: 'gemini',
        label: 'Google Gemini',
        baseUrl: 'https://generativelanguage.googleapis.com/v1beta',
        apiFormat: 'geminiGenerateContent',
        models: ['gemini-3.8-flash'],
    },
    {
        value: 'deepSeek',
        label: 'DeepSeek',
        baseUrl: 'https://api.deepseek.com/v1',
        apiFormat: 'openAiChatCompletions',
        models: ['deepseek-flash'],
    },
    {
        value: 'miniMax',
        label: 'MiniMax',
        baseUrl: 'https://api.minimax.io/v1',
        apiFormat: 'openAiChatCompletions',
        models: ['MiniMax-M3.1-Flash'],
    },
    {
        value: 'moonshot',
        label: 'Moonshot / Kimi',
        baseUrl: 'https://api.moonshot.cn/v1',
        apiFormat: 'openAiChatCompletions',
        models: ['kimi-k3.1-pro'],
    },
    {
        value: 'dashScope',
        label: '阿里云百炼',
        baseUrl: 'https://dashscope.aliyuncs.com/compatible-mode/v1',
        apiFormat: 'openAiChatCompletions',
        models: ['qwen-plus'],
    },
    {
        value: 'openRouter',
        label: 'OpenRouter',
        baseUrl: 'https://openrouter.ai/api/v1',
        apiFormat: 'openAiChatCompletions',
        models: ['~openai/gpt-sol-latest'],
    },
    {
        value: 'ollama',
        label: 'Ollama（本机）',
        baseUrl: 'http://localhost:11434/v1',
        apiFormat: 'openAiChatCompletions',
        models: ['gpt-oss:20b'],
    },
];

function loopbackHostname(hostname: string): boolean {
    return (
        hostname === 'localhost' || hostname === '[::1]' || /^127(?:\.\d{1,3}){3}$/.test(hostname)
    );
}

export function requiresBuiltinApiKey(settings: AssistantBuiltinSettings): boolean {
    if (settings.providerPreset !== 'ollama') return true;
    try {
        const url = new URL(settings.baseUrl.trim());
        return !(['http:', 'https:'].includes(url.protocol) && loopbackHostname(url.hostname));
    } catch {
        return true;
    }
}

export function applyAssistantApiFormat(
    settings: AssistantBuiltinSettings,
    apiFormat: AssistantApiFormat,
): AssistantBuiltinSettings {
    const current = normalizeBuiltinSettings(settings);
    if (current.apiFormat === apiFormat) return current;
    const suffixes = {
        openAiChatCompletions: '/chat/completions',
        openAiResponses: '/responses',
        anthropicMessages: '/messages',
        geminiGenerateContent: null,
    };
    const suffix = suffixes[current.apiFormat];
    let baseUrl = current.baseUrl;
    try {
        const url = new URL(baseUrl);
        const path = url.pathname.replace(/\/+$/, '');
        // 只替换当前协议的标准终点；中转服务的自定义前缀保持原样。
        if (suffix && path.endsWith(suffix) && !url.search && !url.hash) {
            url.pathname = `${path.slice(0, -suffix.length)}${suffixes[apiFormat] ?? ''}` || '/';
            baseUrl = url.toString().replace(/\/+$/, '');
        }
    } catch {
        // 地址仍在编辑时只切格式，URL 校验由表单显示。
    }
    return { ...current, apiFormat, baseUrl };
}

export function applyAssistantModelPreset(
    settings: AssistantBuiltinSettings,
    providerPreset: AssistantModelPreset,
): AssistantBuiltinSettings {
    const current = normalizeBuiltinSettings(settings);
    const next = assistantModelPresets.find((preset) => preset.value === providerPreset)!;
    const previous = assistantModelPresets.find(
        (preset) => preset.value === current.providerPreset,
    );
    const profile =
        providerPreset === 'custom' ? current : applyAssistantApiFormat(current, next.apiFormat);
    const canReplaceUrl =
        !profile.baseUrl ||
        profile.baseUrl.replace(/\/+$/, '') === previous?.baseUrl.replace(/\/+$/, '');
    return {
        ...profile,
        providerPreset,
        baseUrl: providerPreset !== 'custom' && canReplaceUrl ? next.baseUrl : profile.baseUrl,
    };
}

export function validateBuiltinSettings(
    settings: AssistantBuiltinSettings,
): Partial<Record<keyof AssistantBuiltinSettings, string>> {
    const errors: Partial<Record<keyof AssistantBuiltinSettings, string>> = {};
    const value = settings.baseUrl.trim();
    if (value) {
        try {
            const url = new URL(value);
            const loopback = loopbackHostname(url.hostname);
            if (
                (url.protocol !== 'https:' && !(url.protocol === 'http:' && loopback)) ||
                url.username ||
                url.password ||
                value.includes('?') ||
                value.includes('#')
            ) {
                errors.baseUrl =
                    '使用 HTTPS 地址；本机模型可用回环 HTTP，地址不能含账号、查询参数或片段。';
            }
        } catch {
            errors.baseUrl = '请输入有效的服务地址。';
        }
    }
    for (const [name, min, max] of [
        ['maxSteps', 1, 12],
        ['maxOutputTokens', 128, 8192],
        ['requestTimeoutSecs', 10, 180],
    ] as const) {
        const number = settings[name];
        if (!Number.isInteger(number) || number < min || number > max)
            errors[name] = `请输入 ${min}–${max} 的整数。`;
    }
    return errors;
}

export function normalizeBuiltinSettings(
    settings: AssistantBuiltinSettings,
): AssistantBuiltinSettings {
    return {
        ...settings,
        providerPreset: settings.providerPreset ?? 'custom',
        apiFormat: settings.apiFormat ?? 'openAiChatCompletions',
        baseUrl: settings.baseUrl.trim(),
        model: settings.model.trim(),
    };
}

export function modelOrigin(baseUrl: string): string | null {
    try {
        return new URL(baseUrl.trim()).origin;
    } catch {
        return null;
    }
}
