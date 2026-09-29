import { describe, expect, it } from 'vitest';
import type { KoishiInstanceConfig, KoishiPluginNode } from '../../ipc/types';
import {
    appendTo,
    cloneNode,
    freshIdent,
    groupChoices,
    koishiServer,
    moveTo,
    newGroup,
    newPlugin,
    nodeAt,
    nodePathOfIssue,
    replaceAt,
    setServerField,
    validateKoishiConfig,
} from './koishiConfig';

const p = (name: string, ident: string, enabled = true, config: Record<string, unknown> = {}): KoishiPluginNode => ({
    name,
    ident,
    enabled,
    meta: {},
    config,
    children: [],
});
const g = (ident: string, children: KoishiPluginNode[], enabled = true): KoishiPluginNode => ({
    ...p('group', ident, enabled),
    children,
});

const base = (): KoishiInstanceConfig => ({
    global: {},
    entry_meta: {},
    plugins: [
        g('server', [p('server', 'cj4vi7', true, { port: 5140, maxPort: 5149 })]),
        g('basic', [p('help', 'a1'), p('commands', 'c1', false)]),
        g('adapter', []),
    ],
});

describe('koishiConfig', () => {
    it('reads and pins server fields', () => {
        const cfg = base();
        expect(koishiServer(cfg)).toEqual({ port: 5140, host: '127.0.0.1', selfUrl: '' });
        const next = setServerField(cfg, 'port', 23140);
        expect(koishiServer(next).port).toBe(23140);
        expect(nodeAt(next, [0, 0])?.config.maxPort).toBeUndefined();
        expect(koishiServer(setServerField(next, 'selfUrl', 'https://x')).selfUrl).toBe('https://x');
        expect(nodeAt(setServerField(next, 'host', ''), [0, 0])?.config.host).toBeUndefined();
    });

    it('tree edits keep other nodes', () => {
        const cfg = base();
        const off = replaceAt(cfg, [1, 0], (n) => ({ ...n, enabled: false }));
        expect(nodeAt(off, [1, 0])?.enabled).toBe(false);
        expect(nodeAt(cfg, [1, 0])?.enabled).toBe(true);
        const removed = replaceAt(cfg, [1, 1], () => null);
        expect(nodeAt(removed, [1])?.children.map((c) => c.name)).toEqual(['help']);
        const added = appendTo(cfg, [2], newPlugin(cfg, 'adapter-onebot'));
        expect(nodeAt(added, [2, 0])?.name).toBe('adapter-onebot');
        expect(nodeAt(added, [2, 0])?.enabled).toBe(false);
    });

    it('moves by group ident and refuses moving into itself', () => {
        const cfg = base();
        const moved = moveTo(cfg, [1, 0], 'adapter');
        expect(nodeAt(moved, [1])?.children.map((c) => c.name)).toEqual(['commands']);
        expect(nodeAt(moved, [2, 0])?.name).toBe('help');
        const toRoot = moveTo(cfg, [1, 0], '');
        expect(toRoot.plugins.at(-1)?.name).toBe('help');
        expect(moveTo(cfg, [1], 'basic')).toBe(cfg);
        expect(groupChoices(cfg).map((c) => c.ident)).toEqual(['', 'server', 'basic', 'adapter']);
    });

    it('fresh idents avoid collisions', () => {
        const cfg = base();
        let n = 0;
        const seq = [0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.2, 0.2, 0.2, 0.2, 0.2, 0.2];
        const id = freshIdent(cfg, () => seq[n++ % seq.length]);
        expect(id).toHaveLength(6);
        expect(cloneNode(cfg, nodeAt(cfg, [1, 0])!).ident).not.toBe('a1');
        expect(newGroup(cfg, '适配').meta.$label).toBe('适配');
    });

    it('validation mirrors backend paths', () => {
        const cfg = base();
        expect(validateKoishiConfig(cfg)).toEqual([]);
        const dup = appendTo(cfg, [], p('help', 'a1'));
        const issues = validateKoishiConfig(dup);
        expect(issues.some((i) => i.path === 'plugins/3/ident')).toBe(true);
        const bad = appendTo(cfg, [], p('a:b', 'zz'));
        expect(validateKoishiConfig(bad)[0].path).toBe('plugins/3/name');
        expect(validateKoishiConfig(setServerField(cfg, 'port', 0))[0].path).toBe('server/port');
        expect(nodePathOfIssue('plugins/1/children/0/ident')).toEqual([1, 0]);
        expect(nodePathOfIssue('server/port')).toBeNull();
    });
});
