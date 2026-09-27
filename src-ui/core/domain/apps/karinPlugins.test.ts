import { describe, expect, it } from 'vitest';
import { filterKarinPlugins, overlayKarinInstalled } from './karinPlugins';
import type { KarinPluginMarketEntry } from '../../ipc/types';

const market: KarinPluginMarketEntry[] = [
    {
        name: '@karinjs/plugin-basic',
        type: 'npm',
        description: 'basic',
        author: [{ name: 'shijin', home: '' }],
        time: '2025-01-19 10:00:00',
        home: '',
        repo: [],
        files: [],
        allowBuild: [],
    },
    {
        name: 'karin-plugins-alijs',
        type: 'app',
        description: '集合',
        author: [{ name: 'Aliorpse', home: '' }],
        time: '2025-02-16 11:45:14',
        home: '',
        repo: [],
        files: [],
        allowBuild: [],
    },
];

describe('filterKarinPlugins', () => {
    it('filters by query and kind', () => {
        const installed = [
            { name: '@karinjs/plugin-basic', kind: 'npm' as const, version: '1.0.0', enabled: true },
        ];
        const q = filterKarinPlugins(market, installed, 'basic', 'all');
        expect(q).toHaveLength(1);
        expect(q[0].installed).toBe(true);
        expect(filterKarinPlugins(market, installed, '', 'app')).toHaveLength(1);
        expect(filterKarinPlugins(market, installed, '', 'git')).toHaveLength(0);
    });

    it('keeps installed-only plugins', () => {
        const extra = [{ name: 'hand-made.js', kind: 'app' as const, version: undefined, enabled: true }];
        const rows = filterKarinPlugins(market, extra, 'hand', 'all');
        expect(rows[0].name).toBe('hand-made.js');
        expect(rows[0].installed).toBe(true);
    });
});

describe('overlayKarinInstalled', () => {
    it('treats a successful install as installed until scan catches up', () => {
        const rows = filterKarinPlugins(
            market,
            overlayKarinInstalled([], [
                { pluginName: '@karinjs/plugin-basic', action: 'install', status: 'success', atMs: 2 },
            ]),
            '',
            'all',
        );
        expect(rows.find((r) => r.name === '@karinjs/plugin-basic')?.installed).toBe(true);
    });

    it('lets a later uninstall win', () => {
        const scanned = [
            { name: '@karinjs/plugin-basic', kind: 'npm' as const, version: '1.0.0', enabled: true },
        ];
        const next = overlayKarinInstalled(scanned, [
            { pluginName: '@karinjs/plugin-basic', action: 'install', status: 'success', atMs: 1 },
            { pluginName: '@karinjs/plugin-basic', action: 'uninstall', status: 'success', atMs: 2 },
        ]);
        expect(next).toHaveLength(0);
    });
});
