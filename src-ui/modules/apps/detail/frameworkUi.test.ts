import { describe, expect, it } from 'vitest';
import { resolveFrameworkUi } from './frameworkUi';

describe('resolveFrameworkUi', () => {
    it('unknown framework has no extra tabs (raw + log only)', () => {
        expect(resolveFrameworkUi('koishi')).toBeUndefined();
    });

    it('astrbot extra tabs and issue routing', () => {
        const ui = resolveFrameworkUi('astrbot');
        expect(ui).toBeDefined();
        expect(ui?.defaultTab).toBe('plugins');
        expect(ui?.extraTabs.map((t) => t.value)).toEqual(['plugins', 'connections']);
        expect(ui?.typedTabs.has('connections')).toBe(true);
        expect(ui?.fillPaneTabs.has('plugins')).toBe(true);
        expect(ui?.tabForIssue('onebot/ws_reverse_port')).toBe('connections');
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
