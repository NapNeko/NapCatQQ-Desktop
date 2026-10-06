import { describe, expect, it } from 'vitest';
import {
    catalogTimeLabel,
    isPluginTaskOf,
    matchesCatalogQuery,
    overlayInstalledFromTasks,
    parseCatalogTime,
    pluginCatalogErrorCopy,
    pluginTaskHint,
    type AppPluginTask,
} from './pluginCatalog';
import { KARIN_CATALOG } from './karinPlugins';
import type { DeploymentTaskSnapshot } from '../../ipc/types';

function pluginTask(
    over: Partial<DeploymentTaskSnapshot> & { resource?: 'plugin' | 'adapter' },
): DeploymentTaskSnapshot {
    const { resource = 'plugin', ...rest } = over;
    return {
        taskId: 't1',
        kind: {
            kind: 'app_plugin',
            instance_id: 'i1',
            plugin_name: 'foo',
            action: 'install',
            resource,
        },
        status: 'running',
        hostId: 'local',
        title: '',
        resources: [],
        progressEvents: [],
        submittedAtMs: 1n,
        cancellable: false,
        ...rest,
    };
}

describe('parseCatalogTime', () => {
    it('parses official space-separated time', () => {
        expect(parseCatalogTime('2025-01-19 10:00:00')).not.toBeNull();
        expect(parseCatalogTime('2025-01-19T10:00:00Z')).not.toBeNull();
        expect(parseCatalogTime('')).toBeNull();
        expect(parseCatalogTime('whenever')).toBeNull();
    });
});

describe('catalogTimeLabel', () => {
    it('shows the version once installed', () => {
        expect(catalogTimeLabel('2025-01-19 10:00:00', true, '1.2.0')).toBe('v1.2.0');
        expect(catalogTimeLabel('', true)).toBe('已装');
    });

    it('drops listings older than maxDays', () => {
        expect(catalogTimeLabel('2001-01-01 00:00:00', false, undefined, 7)).toBeNull();
        expect(catalogTimeLabel('2001-01-01 00:00:00', false)).toMatch(/年前$/);
    });
});

describe('matchesCatalogQuery', () => {
    it('matches any field case-insensitively and keeps everything on empty search', () => {
        expect(matchesCatalogQuery('  ', ['a'])).toBe(true);
        expect(matchesCatalogQuery('BASIC', ['@karinjs/plugin-basic', ''])).toBe(true);
        expect(matchesCatalogQuery('zzz', ['a', 'b'])).toBe(false);
    });
});

describe('overlayInstalledFromTasks', () => {
    const byName = (item: { name: string }, name: string) => item.name === name;
    const make = (name: string) => ({ name });

    it('lets the latest task per name win', () => {
        const next = overlayInstalledFromTasks(
            [],
            [
                { pluginName: 'a', action: 'uninstall', status: 'success', atMs: 1 },
                { pluginName: 'a', action: 'install', status: 'success', atMs: 2 },
            ],
            byName,
            make,
        );
        expect(next).toEqual([{ name: 'a' }]);
    });

    it('ignores tasks that did not succeed', () => {
        const next = overlayInstalledFromTasks(
            [{ name: 'a' }],
            [
                { pluginName: 'a', action: 'uninstall', status: 'failed', atMs: 1 },
                { pluginName: 'b', action: 'install', status: 'running', atMs: 1 },
            ],
            byName,
            make,
        );
        expect(next).toEqual([{ name: 'a' }]);
    });
});

describe('plugin tasks', () => {
    it('picks tasks of this instance and resource; missing resource counts as plugin', () => {
        const legacy = pluginTask({});
        (legacy.kind as { resource?: unknown }).resource = undefined;
        expect(isPluginTaskOf(legacy, 'i1', 'plugin')).toBe(true);
        expect(isPluginTaskOf(pluginTask({ resource: 'adapter' }), 'i1', 'plugin')).toBe(false);
        expect(isPluginTaskOf(pluginTask({}), 'other', 'plugin')).toBe(false);
    });

    it('dates a hint by end, then start, then submit', () => {
        const task = pluginTask({ submittedAtMs: 1n, startedAtMs: 5n }) as AppPluginTask;
        expect(pluginTaskHint(task).atMs).toBe(5);
        expect(pluginTaskHint({ ...task, endedAtMs: 9n }).atMs).toBe(9);
        expect(pluginTaskHint({ ...task, startedAtMs: null }).atMs).toBe(1);
    });
});

describe('pluginCatalogErrorCopy', () => {
    it('tells network trouble apart and still points to the logs', () => {
        const copy = pluginCatalogErrorCopy(
            '写入应用端配置失败: 拉取插件目录失败: error sending request for url (https://registry.npmjs.com/@karinjs/plugins-list/latest)',
            KARIN_CATALOG,
        );
        expect(copy.title).toBe('无法连接官方插件目录');
        expect(copy.content).toContain('检查网络或代理后重试');
        expect(copy.content).toContain('详情见日志');
    });

    it('uses the neutral name for the app store', () => {
        expect(pluginCatalogErrorCopy('拉取插件目录失败: error sending request').title).toBe(
            '无法连接官方目录',
        );
        expect(pluginCatalogErrorCopy('HTTP 503').title).toBe('官方目录暂时不可用');
    });

    it('keeps other failures to a pointer at the logs', () => {
        expect(pluginCatalogErrorCopy('解析目录 JSON 失败: expected value')).toEqual({
            title: '目录加载失败',
            content: '详情见日志',
        });
    });
});
