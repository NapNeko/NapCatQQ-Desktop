// 内置模型配置的表单校验，不发起网络请求。

import type { AssistantBuiltinSettings } from './types';

export function validateBuiltinSettings(
    settings: AssistantBuiltinSettings,
): Partial<Record<keyof AssistantBuiltinSettings, string>> {
    const errors: Partial<Record<keyof AssistantBuiltinSettings, string>> = {};
    const value = settings.baseUrl.trim();
    if (value) {
        try {
            const url = new URL(value);
            const loopback =
                url.hostname === 'localhost' ||
                url.hostname === '[::1]' ||
                /^127(?:\.\d{1,3}){3}$/.test(url.hostname);
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

export function modelOrigin(baseUrl: string): string | null {
    try {
        return new URL(baseUrl.trim()).origin;
    } catch {
        return null;
    }
}
