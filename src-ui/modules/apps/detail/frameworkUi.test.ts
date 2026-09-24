import { describe, expect, it } from 'vitest';
import { buildDetailNav, resolveFrameworkUi, type FrameworkUiModule } from './frameworkUi';

const navValues = (ui: FrameworkUiModule | undefined) => (ui?.nav ?? []).flatMap((g) => g.items.map((t) => t.value));

describe('resolveFrameworkUi', () => {
    it('unknown framework has no module (raw + log only)', () => {
        expect(resolveFrameworkUi('koishi')).toBeUndefined();
    });

    it('astrbot nav groups and issue routing', () => {
        const ui = resolveFrameworkUi('astrbot');
        expect(ui).toBeDefined();
        expect(ui?.defaultTab).toBe('overview');
        expect(ui?.nav.map((g) => g.id)).toEqual(['home', 'ai', 'message', 'extend', 'instance']);
        expect(navValues(ui)).toEqual([
            'overview',
            'models',
            'persona',
            'kb',
            'subagent',
            'talk',
            'rules',
            'plugins',
            'connections',
        ]);
        // 概览、人格、知识库也能就地改配置，保存条要在；只有插件页不是
        for (const t of ['overview', 'connections', 'models', 'talk', 'persona', 'kb', 'subagent', 'rules']) {
            expect(ui?.typedTabs.has(t)).toBe(true);
        }
        expect(ui?.typedTabs.has('plugins')).toBe(false);
        expect(ui?.fillPaneTabs.has('plugins')).toBe(true);
        expect(ui?.tabForIssue('onebot/ws_reverse_port')).toBe('connections');
        expect(ui?.tabForIssue('sources/0/id')).toBe('models');
        expect(ui?.tabForIssue('ai/default_provider_id')).toBe('models');
        expect(ui?.tabForIssue('ai/fallback_chat_models/0')).toBe('models');
        expect(ui?.tabForIssue('ai/default_personality')).toBe('persona');
        expect(ui?.tabForIssue('kb/fusion_top_k')).toBe('kb');
        expect(ui?.tabForIssue('ai/max_agent_step')).toBe('talk');
        expect(ui?.tabForIssue('gates/id_whitelist')).toBe('talk');
        expect(ui?.tabForIssue('subagent/main_enable')).toBe('subagent');
    });

    it('karin nav groups and issue routing', () => {
        const ui = resolveFrameworkUi('karin');
        expect(ui).toBeDefined();
        expect(ui?.defaultTab).toBe('basic');
        expect(ui?.nav.map((g) => g.id)).toEqual(['config', 'extend', 'instance']);
        expect(navValues(ui)).toEqual(['basic', 'permissions', 'rules', 'render', 'plugins', 'connections']);
        expect(ui?.typedTabs.has('connections')).toBe(true);
        expect(ui?.typedTabs.has('plugins')).toBe(false);
        expect(ui?.fillPaneTabs.has('plugins')).toBe(true);
        expect(ui?.tabForIssue('env/http_port')).toBe('connections');
        expect(ui?.tabForIssue('groups/0')).toBe('rules');
        expect(ui?.tabForIssue('config/master')).toBe('permissions');
    });

    it('nonebot2 nav groups and issue routing', () => {
        const ui = resolveFrameworkUi('nonebot2');
        expect(ui).toBeDefined();
        expect(ui?.defaultTab).toBe('adapters');
        expect(ui?.nav.map((g) => g.id)).toEqual(['extend', 'instance']);
        expect(navValues(ui)).toEqual(['adapters', 'plugins', 'connections']);
        expect(ui?.typedTabs.has('connections')).toBe(true);
        expect(ui?.fillPaneTabs.has('adapters')).toBe(true);
        expect(ui?.tabForIssue('env_prod/port')).toBe('connections');
    });
});

describe('buildDetailNav', () => {
    it('unknown framework gets only the instance group with raw + log', () => {
        expect(buildDetailNav(undefined)).toEqual([
            {
                id: 'instance',
                label: '实例',
                items: [
                    { value: 'raw', label: '原始文件' },
                    { value: 'log', label: '日志' },
                ],
            },
        ]);
    });

    it('appends raw + log to the framework instance group without touching the module', () => {
        const ui = resolveFrameworkUi('karin');
        const before = navValues(ui);
        const nav = buildDetailNav(ui);
        expect(nav.at(-1)?.items.map((t) => t.value)).toEqual(['connections', 'raw', 'log']);
        expect(nav.flatMap((g) => g.items).filter((t) => t.value === 'raw')).toHaveLength(1);
        buildDetailNav(ui);
        expect(navValues(ui)).toEqual(before);
    });

    it('creates the instance group when a framework has none', () => {
        const ui = { ...resolveFrameworkUi('karin')!, nav: [{ id: 'x', items: [{ value: 'a', label: 'A' }] }] };
        expect(buildDetailNav(ui).map((g) => g.id)).toEqual(['x', 'instance']);
    });
});
