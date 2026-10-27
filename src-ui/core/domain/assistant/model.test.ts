import { describe, expect, it } from 'vitest';
import type { AssistantBuiltinSettings } from './types';
import {
    applyAssistantApiFormat,
    applyAssistantModelPreset,
    modelOrigin,
    normalizeBuiltinSettings,
    requiresBuiltinApiKey,
    validateBuiltinSettings,
} from './model';

const defaults: AssistantBuiltinSettings = {
    providerPreset: 'custom',
    apiFormat: 'openAiChatCompletions',
    baseUrl: '',
    model: '',
    maxSteps: 6,
    maxOutputTokens: 2048,
    requestTimeoutSecs: 90,
};

describe('内置模型配置', () => {
    it.each([
        [
            'openAiChatCompletions',
            'https://proxy.example/v1/chat/completions',
            'openAiResponses',
            'https://proxy.example/v1/responses',
        ],
        [
            'openAiChatCompletions',
            'https://proxy.example/proxy/chat/completions',
            'openAiResponses',
            'https://proxy.example/proxy/responses',
        ],
        [
            'openAiResponses',
            'https://proxy.example/api/v1/responses/',
            'anthropicMessages',
            'https://proxy.example/api/v1/messages',
        ],
        [
            'anthropicMessages',
            'https://proxy.example/v1/messages',
            'openAiChatCompletions',
            'https://proxy.example/v1/chat/completions',
        ],
        [
            'openAiChatCompletions',
            'https://proxy.example/proxy/chat/completions',
            'geminiGenerateContent',
            'https://proxy.example/proxy',
        ],
        [
            'openAiChatCompletions',
            'https://proxy.example/custom/inference',
            'openAiResponses',
            'https://proxy.example/custom/inference',
        ],
        [
            'geminiGenerateContent',
            'https://generativelanguage.googleapis.com/v1beta',
            'openAiChatCompletions',
            'https://generativelanguage.googleapis.com/v1beta',
        ],
    ] as const)(
        '显式切换 %s 的 %s 时仅替换当前格式标准终点',
        (apiFormat, baseUrl, next, expectedUrl) => {
            expect(
                applyAssistantApiFormat(
                    { ...defaults, apiFormat, baseUrl, model: 'my-model' },
                    next,
                ),
            ).toMatchObject({ apiFormat: next, baseUrl: expectedUrl, model: 'my-model' });
        },
    );
    it('不改格式时保留完整终点，切预设格式时复用相同规则', () => {
        const current = {
            ...defaults,
            baseUrl: 'https://proxy.example/v1/chat/completions',
            model: 'proxy-model',
        };
        expect(applyAssistantApiFormat(current, 'openAiChatCompletions').baseUrl).toBe(
            current.baseUrl,
        );
        expect(applyAssistantModelPreset(current, 'anthropic')).toMatchObject({
            apiFormat: 'anthropicMessages',
            baseUrl: 'https://proxy.example/v1/messages',
            model: 'proxy-model',
        });
    });
    it('旧配置回落自定义和 Chat Completions，保留手动模型与地址', () => {
        const legacy = { ...defaults, baseUrl: ' https://proxy.example/v1 ', model: ' my-model ' };
        Reflect.deleteProperty(legacy, 'providerPreset');
        Reflect.deleteProperty(legacy, 'apiFormat');
        expect(normalizeBuiltinSettings(legacy)).toMatchObject({
            providerPreset: 'custom',
            apiFormat: 'openAiChatCompletions',
            baseUrl: 'https://proxy.example/v1',
            model: 'my-model',
        });
    });
    it('预设填入默认地址和推荐格式，模型由用户手填或选择', () => {
        const openAi = applyAssistantModelPreset(defaults, 'openAi');
        expect(openAi).toMatchObject({
            providerPreset: 'openAi',
            baseUrl: 'https://api.openai.com/v1',
            apiFormat: 'openAiResponses',
            model: '',
        });
        expect(applyAssistantModelPreset(openAi, 'anthropic')).toMatchObject({
            providerPreset: 'anthropic',
            baseUrl: 'https://api.anthropic.com/v1',
            apiFormat: 'anthropicMessages',
        });
    });
    it('切换预设保留手动地址与模型，自定义保留当前格式', () => {
        const manual = { ...defaults, baseUrl: 'https://proxy.example/custom', model: 'my-model' };
        const gemini = applyAssistantModelPreset(manual, 'gemini');
        expect(gemini).toMatchObject({
            baseUrl: manual.baseUrl,
            model: manual.model,
            apiFormat: 'geminiGenerateContent',
        });
        expect(applyAssistantModelPreset(gemini, 'custom')).toMatchObject({
            baseUrl: manual.baseUrl,
            model: manual.model,
            apiFormat: 'geminiGenerateContent',
        });
    });
    it.each(['http://localhost:11434/v1', 'http://127.0.0.1:11434/v1', 'https://[::1]:11434/v1'])(
        '仅本机 Ollama 可省略 Key：%s',
        (baseUrl) => {
            expect(requiresBuiltinApiKey({ ...defaults, providerPreset: 'ollama', baseUrl })).toBe(
                false,
            );
            expect(requiresBuiltinApiKey({ ...defaults, baseUrl })).toBe(true);
        },
    );
    it.each([
        'https://ollama.com/v1',
        'http://127.attacker.example/v1',
        'ftp://localhost/v1',
        'invalid',
    ])('非本机 Ollama 仍需 Key：%s', (baseUrl) => {
        expect(requiresBuiltinApiKey({ ...defaults, providerPreset: 'ollama', baseUrl })).toBe(
            true,
        );
    });
    it('允许尚未配置的新安装与只保存地址或模型名称的草稿', () => {
        expect(validateBuiltinSettings(defaults)).toEqual({});
        expect(
            validateBuiltinSettings({ ...defaults, baseUrl: 'https://models.example/v1' }),
        ).toEqual({});
        expect(validateBuiltinSettings({ ...defaults, model: 'user-selected-model' })).toEqual({});
    });
    it.each([
        'https://models.example/v1',
        'http://localhost:1234/v1',
        'http://127.20.0.1:8000/v1',
        'http://[::1]:1234/v1',
    ])('接受受支持地址 %s', (baseUrl) => {
        expect(validateBuiltinSettings({ ...defaults, baseUrl }).baseUrl).toBeUndefined();
    });
    it.each([
        'http://models.example/v1',
        'http://127.attacker.example/v1',
        'https://name:password@models.example/v1',
        'https://models.example/v1?key=value',
        'https://models.example/v1?',
        'https://models.example/v1#fragment',
        'ftp://models.example/v1',
    ])('拒绝不适用的地址 %s', (baseUrl) => {
        expect(validateBuiltinSettings({ ...defaults, baseUrl }).baseUrl).toBeTruthy();
    });
    it('预算越界或小数会阻止保存，不影响其他字段', () => {
        expect(
            validateBuiltinSettings({
                ...defaults,
                maxSteps: 13,
                maxOutputTokens: 127,
                requestTimeoutSecs: 9,
            }),
        ).toMatchObject({
            maxSteps: expect.any(String),
            maxOutputTokens: expect.any(String),
            requestTimeoutSecs: expect.any(String),
        });
        expect(validateBuiltinSettings({ ...defaults, maxSteps: 1.5 }).maxSteps).toBeTruthy();
        expect(
            validateBuiltinSettings({
                ...defaults,
                maxSteps: 12,
                maxOutputTokens: 8192,
                requestTimeoutSecs: 180,
            }),
        ).toEqual({});
    });
    it('Key 对应服务 origin，路径变化保持同源，换主机或端口不同源', () => {
        expect(modelOrigin('https://models.example:443/v1')).toBe(
            modelOrigin('https://models.example/custom'),
        );
        expect(modelOrigin('https://other.example/v1')).not.toBe(
            modelOrigin('https://models.example/v1'),
        );
        expect(modelOrigin('http://localhost:1234/v1')).not.toBe(
            modelOrigin('http://localhost:1235/v1'),
        );
    });
});
