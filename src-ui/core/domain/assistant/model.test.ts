import { describe, expect, it } from 'vitest';
import type { AssistantBuiltinSettings } from './types';
import { modelOrigin, validateBuiltinSettings } from './model';

const defaults: AssistantBuiltinSettings = {
    baseUrl: '',
    model: '',
    maxSteps: 6,
    maxOutputTokens: 2048,
    requestTimeoutSecs: 90,
};

describe('内置模型配置', () => {
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
