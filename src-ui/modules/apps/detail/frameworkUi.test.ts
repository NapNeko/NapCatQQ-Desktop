import { describe, expect, it } from 'vitest';
import { resolveFrameworkUi } from './frameworkUi';

describe('resolveFrameworkUi', () => {
    it('unknown framework has no extra tabs (raw + log only)', () => {
        expect(resolveFrameworkUi('koishi')).toBeUndefined();
    });

    it('astrbot extra tabs and issue routing', () => {
        const ui = resolveFrameworkUi('astrbot');
        expect(ui).toBeDefined();
        expect(ui?.defaultTab).toBe('connections');
        expect(ui?.extraTabs.map((t) => t.value)).toEqual([
            'connections',
            'models',
            'talk',
            'persona',
            'kb',
            'subagent',
            'rules',
            'plugins',
        ]);
        // 人格 / 知识库页也能改配置（设默认、挂载），保存条要一直在；只有插件页不是
        for (const t of ['connections', 'models', 'talk', 'persona', 'kb', 'subagent', 'rules']) {
            expect(ui?.typedTabs.has(t)).toBe(true);
        }
        expect(ui?.typedTabs.has('plugins')).toBe(false);
        expect(ui?.fillPaneTabs.has('plugins')).toBe(true);
        expect(ui?.tabForIssue('onebot/ws_reverse_port')).toBe('connections');
        expect(ui?.tabForIssue('sources/0/id')).toBe('models');
        expect(ui?.tabForIssue('gates/id_whitelist')).toBe('talk');
        expect(ui?.tabForIssue('subagent/main_enable')).toBe('subagent');
    });

    it('karin extra tabs and issue routing', () => {
        const ui = resolveFrameworkUi('karin');
        expect(ui).toBeDefined();
        expect(ui?.defaultTab).toBe('basic');
        expect(ui?.extraTabs.map((t) => t.value)).toEqual([
            'basic',
            'permissions',
            'connections',
            'rules',
            'render',
            'plugins',
        ]);
        expect(ui?.typedTabs.has('connections')).toBe(true);
        expect(ui?.typedTabs.has('plugins')).toBe(false);
        expect(ui?.fillPaneTabs.has('plugins')).toBe(true);
        expect(ui?.tabForIssue('env/http_port')).toBe('connections');
        expect(ui?.tabForIssue('groups/0')).toBe('rules');
        expect(ui?.tabForIssue('config/master')).toBe('permissions');
    });

    it('nonebot2 extra tabs and issue routing', () => {
        const ui = resolveFrameworkUi('nonebot2');
        expect(ui).toBeDefined();
        expect(ui?.defaultTab).toBe('adapters');
        expect(ui?.extraTabs.map((t) => t.value)).toEqual(['adapters', 'plugins', 'connections']);
        expect(ui?.typedTabs.has('connections')).toBe(true);
        expect(ui?.fillPaneTabs.has('adapters')).toBe(true);
        expect(ui?.tabForIssue('env_prod/port')).toBe('connections');
    });
});
