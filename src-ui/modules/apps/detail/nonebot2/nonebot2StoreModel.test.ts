import { describe, expect, it } from 'vitest';
import {
    filterNoneBot2Store,
    overlayInstalledFromTasks,
    paginateStore,
    pluginCatalogErrorCopy,
    storeGridFit,
    storeOpErrorCopy,
    storePageItems,
} from './nonebot2StoreModel';
import type { AppStoreMarketEntry } from '../../../../core/ipc/types';

const adapters: AppStoreMarketEntry[] = [
    {
        resource: 'adapter',
        id: 'nonebot.adapters.onebot.v11',
        name: 'OneBot V11',
        description: 'v11',
        version: '2.4.6',
        author: 'yanyongyu',
        homepage: '',
        time: '2025-01-01 00:00:00',
        package: 'nonebot-adapter-onebot',
        module_name: 'nonebot.adapters.onebot.v11',
        flavor: 'pypi',
        is_official: true,
        valid: true,
        tags: [],
        supported_adapters: [],
        authors: [],
        repos: [],
        files: [],
        allow_build: [],
    },
    {
        resource: 'adapter',
        id: 'nonebot.adapters.console',
        name: 'Console',
        description: 'console',
        version: '',
        author: '',
        homepage: '',
        time: '',
        package: 'nonebot-adapter-console',
        module_name: 'nonebot.adapters.console',
        flavor: 'pypi',
        is_official: true,
        valid: true,
        tags: [],
        supported_adapters: [],
        authors: [],
        repos: [],
        files: [],
        allow_build: [],
    },
];

const plugins: AppStoreMarketEntry[] = [
    {
        resource: 'plugin',
        id: 'nonebot_plugin_foo',
        name: 'Foo',
        description: 'foo plugin',
        version: '',
        author: 'a',
        homepage: '',
        time: '',
        package: 'nonebot-plugin-foo',
        module_name: 'nonebot_plugin_foo',
        flavor: 'pypi',
        is_official: false,
        valid: true,
        tags: [],
        supported_adapters: ['nonebot.adapters.onebot.v11'],
        authors: [],
        repos: [],
        files: [],
        allow_build: [],
    },
    {
        resource: 'plugin',
        id: 'nonebot_plugin_bar',
        name: 'Bar',
        description: 'bar plugin',
        version: '',
        author: 'b',
        homepage: '',
        time: '',
        package: 'nonebot-plugin-bar',
        module_name: 'nonebot_plugin_bar',
        flavor: 'pypi',
        is_official: false,
        valid: true,
        tags: [],
        supported_adapters: ['nonebot.adapters.telegram'],
        authors: [],
        repos: [],
        files: [],
        allow_build: [],
    },
];

describe('filterNoneBot2Store', () => {
    it('shows all adapters even with empty search', () => {
        const rows = filterNoneBot2Store({
            resource: 'adapter',
            entries: adapters,
            installed: [
                {
                    id: 'nonebot.adapters.onebot.v11',
                    name: 'OneBot V11',
                    resource: 'adapter',
                    flavor: 'pypi',
                    enabled: true,
                    package: 'nonebot-adapter-onebot',
                },
            ],
            query: '',
            kindFilter: 'all',
            enabledAdapterModules: ['nonebot.adapters.onebot.v11'],
            linked: true,
        });
        expect(rows).toHaveLength(2);
        expect(rows.find((r) => r.id.endsWith('v11'))?.locked).toBe(true);
        expect(rows.find((r) => r.id.endsWith('v11'))?.installed).toBe(true);
    });

    it('shows plugin catalog without search', () => {
        const rows = filterNoneBot2Store({
            resource: 'plugin',
            entries: plugins,
            installed: [
                {
                    id: 'nonebot_plugin_foo',
                    name: 'Foo',
                    resource: 'plugin',
                    flavor: 'pypi',
                    enabled: true,
                    package: 'nonebot-plugin-foo',
                },
            ],
            query: '',
            kindFilter: 'all',
            enabledAdapterModules: ['nonebot.adapters.onebot.v11'],
            linked: false,
        });
        expect(rows.map((r) => r.id)).toEqual(['nonebot_plugin_foo']);
        expect(rows[0]?.installed).toBe(true);
    });

    it('plugin search filters by supported adapters', () => {
        const rows = filterNoneBot2Store({
            resource: 'plugin',
            entries: plugins,
            installed: [],
            query: 'plugin',
            kindFilter: 'all',
            enabledAdapterModules: ['nonebot.adapters.onebot.v11'],
            linked: false,
        });
        expect(rows.map((r) => r.id)).toEqual(['nonebot_plugin_foo']);
    });
});

describe('overlayInstalledFromTasks', () => {
    it('marks install success immediately', () => {
        const next = overlayInstalledFromTasks(
            [],
            [{ pluginName: 'nonebot_plugin_foo', action: 'install', status: 'success', atMs: 1 }],
            'plugin',
        );
        expect(next[0]?.id).toBe('nonebot_plugin_foo');
        expect(next[0]?.enabled).toBe(true);
    });
});

describe('paginateStore', () => {
    it('pages twelve items like the official store', () => {
        const rows = Array.from({ length: 25 }, (_, i) => i);
        expect(paginateStore(rows, 0)).toEqual(rows.slice(0, 12));
        expect(paginateStore(rows, 2)).toEqual(rows.slice(24));
    });

    it('accepts a fitted page size', () => {
        const rows = Array.from({ length: 20 }, (_, i) => i);
        expect(paginateStore(rows, 0, 9)).toEqual(rows.slice(0, 9));
        expect(paginateStore(rows, 2, 9)).toEqual(rows.slice(18));
    });
});

describe('storeGridFit', () => {
    it('falls back to 3×4 when the pane is not measured', () => {
        expect(storeGridFit(0, 0)).toEqual({ cols: 3, rows: 4, pageSize: 12 });
    });

    it('drops a row when four cards would overflow', () => {
        expect(storeGridFit(1000, 520)).toEqual({ cols: 3, rows: 3, pageSize: 9 });
    });

    it('opens a fourth column on a wide pane', () => {
        expect(storeGridFit(1300, 700)).toEqual({ cols: 4, rows: 4, pageSize: 16 });
    });

    it('keeps three by four on a tall medium pane', () => {
        expect(storeGridFit(1000, 700)).toEqual({ cols: 3, rows: 4, pageSize: 12 });
    });
});

describe('storePageItems', () => {
    it('keeps a wide window plus first and last', () => {
        expect(storePageItems(0, 51)).toEqual([0, 1, 2, 3, 4, 5, 6, 7, 'gap', 50]);
        expect(storePageItems(5, 51)).toEqual([0, 'gap', 2, 3, 4, 5, 6, 7, 8, 'gap', 50]);
        expect(storePageItems(50, 51)).toEqual([0, 'gap', 43, 44, 45, 46, 47, 48, 49, 50]);
    });
});

describe('pluginCatalogErrorCopy', () => {
    it('humanizes network errors', () => {
        const copy = pluginCatalogErrorCopy('拉取插件目录失败: error sending request');
        expect(copy.title).toBe('无法连接官方目录');
        expect(copy.content).toContain('详情见日志');
    });
});

describe('storeOpErrorCopy', () => {
    it('drops the uv frozen hint', () => {
        expect(
            storeOpErrorCopy(
                '应用端运行失败: uv add: exit=Some(1): hint: If you want to add the package regardless of the failed resolution, provide the `--frozen` flag',
            ),
        ).toBe('依赖无法安装，当前环境没有可用的预编译包，或这个插件过旧');
    });

    it('names the package that failed to build', () => {
        expect(storeOpErrorCopy('× Failed to build `pillow==9.5.0`')).toBe(
            '依赖 pillow 9.5.0 无法编译，当前环境没有可用的预编译包',
        );
    });
});
