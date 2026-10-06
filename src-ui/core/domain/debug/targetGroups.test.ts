import { describe, expect, it } from 'vitest';
import type { DebugTarget } from '../../ipc/generated/debug/DebugTarget';
import {
    backendShortLabel,
    defaultTargetId,
    filterTargets,
    groupTargets,
    targetDisplayName,
} from './targetGroups';

function target(bot_id: string, patch: Partial<DebugTarget> = {}): DebugTarget {
    return {
        bot_id,
        name: bot_id,
        qq_id: 10000,
        backend: 'napcat',
        host: { kind: 'local' },
        running: false,
        online: null,
        ...patch,
    };
}

describe('groupTargets', () => {
    it('按宿主分组，本机在最前，其余按出现顺序', () => {
        const list = [
            target('r1', { host: { kind: 'remote', server_id: 'srv-1' } }),
            target('d1', { host: { kind: 'docker', server_id: 'srv-1' } }),
            target('l1'),
            target('r2', { host: { kind: 'remote', server_id: 'srv-1' } }),
            target('l2'),
        ];
        const groups = groupTargets(list, (id) => (id === 'srv-1' ? '家里的 NAS' : undefined));
        expect(groups.map((g) => [g.key, g.label, g.targets.map((t) => t.bot_id)])).toEqual([
            ['local', '本机', ['l1', 'l2']],
            ['remote:srv-1', '远端 · 家里的 NAS', ['r1', 'r2']],
            ['docker:srv-1', 'Docker · 家里的 NAS', ['d1']],
        ]);
    });

    it('远端没有名字时显示 id', () => {
        const groups = groupTargets([
            target('r', { host: { kind: 'remote', server_id: 'srv-9' } }),
        ]);
        expect(groups[0].label).toBe('远端 · srv-9');
    });
});

describe('filterTargets', () => {
    const list = [
        target('mock-a', { name: '小雪', qq_id: 2854196310 }),
        target('mock-b', { name: 'NapCat 测试号', qq_id: 1919810 }),
    ];

    it('空关键字原样返回', () => {
        expect(filterTargets(list, '  ').map((t) => t.bot_id)).toEqual(['mock-a', 'mock-b']);
    });

    it('按名字（不分大小写）、QQ 号、bot_id 匹配', () => {
        expect(filterTargets(list, 'napcat').map((t) => t.bot_id)).toEqual(['mock-b']);
        expect(filterTargets(list, '28541').map((t) => t.bot_id)).toEqual(['mock-a']);
        expect(filterTargets(list, 'MOCK-B').map((t) => t.bot_id)).toEqual(['mock-b']);
        expect(filterTargets(list, '不存在')).toEqual([]);
    });
});

describe('defaultTargetId', () => {
    it('优先在跑的，其次第一个，空列表是 null', () => {
        expect(defaultTargetId([target('a'), target('b', { running: true })])).toBe('b');
        expect(defaultTargetId([target('a'), target('b')])).toBe('a');
        expect(defaultTargetId([])).toBeNull();
    });
});

describe('backendShortLabel', () => {
    it('两个字母', () => {
        expect(backendShortLabel('napcat')).toBe('NC');
        expect(backendShortLabel('snowluma')).toBe('SL');
    });
});

describe('targetDisplayName', () => {
    it('没起名字时用 QQ 号，再没有用 bot_id', () => {
        expect(targetDisplayName(target('a', { name: ' 小雪 ' }))).toBe('小雪');
        expect(targetDisplayName(target('a', { name: '  ', qq_id: 123 }))).toBe('123');
        expect(targetDisplayName(target('a', { name: '', qq_id: 0 }))).toBe('a');
    });
});
