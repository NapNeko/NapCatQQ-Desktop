import { describe, expect, it } from 'vitest';
import { configDataOf, configSaveSummary, issuesByPath, wrapConfigData } from './appConfigForm';
import { KARIN_CONFIG_FORM, karinDefaultConfig } from './karinConfig';
import { NONEBOT2_CONFIG_FORM, nonebot2DefaultConfig } from './nonebot2Config';
import { ASTRBOT_CONFIG_FORM } from './astrbotConfig';
import { MAIBOT_CONFIG_FORM } from './maibotConfig';
import type { AppConfigWriteResult } from '../../ipc/types';

function result(over: Partial<AppConfigWriteResult> = {}): AppConfigWriteResult {
    return {
        config: { framework: 'nonebot2', data: nonebot2DefaultConfig(8080) },
        revision: 'r1',
        documents: [],
        restart_required: false,
        relinked: false,
        port_changed: false,
        ...over,
    };
}

function summary(
    spec: { saveHint: (r: AppConfigWriteResult, running: boolean) => string | null },
    r: AppConfigWriteResult,
    running = false,
): string {
    return configSaveSummary(r, spec.saveHint(r, running));
}

describe('issuesByPath', () => {
    it('keeps the first message per path', () => {
        expect(
            issuesByPath([
                { path: 'a', message: '一' },
                { path: 'a', message: '二' },
                { path: 'b', message: '三' },
            ]),
        ).toEqual({ a: '一', b: '三' });
    });
});

describe('configDataOf / wrapConfigData', () => {
    it('only hands out data of the asked framework', () => {
        const karin = wrapConfigData('karin', karinDefaultConfig());
        expect(configDataOf(karin, 'karin')).toBe(karin.data);
        expect(configDataOf(karin, 'nonebot2')).toBeNull();
    });
});

describe('save summary per framework', () => {
    it('Karin always says how the change lands', () => {
        expect(summary(KARIN_CONFIG_FORM, result())).toBe('Karin 会自动热加载，无需重启');
        expect(
            summary(
                KARIN_CONFIG_FORM,
                result({ port_changed: true, relinked: true, restart_required: true }),
            ),
        ).toBe('实例端口已同步；已同步更新协议 Bot 侧的对接连接；有改动需重启实例后生效');
    });

    it('NoneBot2 and MaiBot fall back to 已保存 when nothing else happened', () => {
        expect(summary(NONEBOT2_CONFIG_FORM, result())).toBe('已保存');
        expect(summary(NONEBOT2_CONFIG_FORM, result({ restart_required: true }))).toBe(
            '改完要重启',
        );
        expect(summary(MAIBOT_CONFIG_FORM, result())).toBe('已保存');
        expect(summary(MAIBOT_CONFIG_FORM, result({ port_changed: true }))).toBe('实例端口已同步');
    });

    it('AstrBot tells hot reload from a plain write', () => {
        expect(summary(ASTRBOT_CONFIG_FORM, result(), true)).toBe('已热生效');
        expect(summary(ASTRBOT_CONFIG_FORM, result(), false)).toBe('已写入');
        expect(summary(ASTRBOT_CONFIG_FORM, result({ restart_required: true }), true)).toBe(
            '改完要重启',
        );
        expect(ASTRBOT_CONFIG_FORM.confId).toBe('default');
    });
});
