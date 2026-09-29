import { describe, expect, it } from 'vitest';
import schemas from '../../ipc/mock/koishi-schemas.json';
import {
    blankOf,
    describe as describeNode,
    hydrateSchema,
    missingRequired,
    objectFields,
    objectSections,
    renderable,
    schemaText,
    taggedBranch,
    unionShape,
    visibleBranches,
} from './koishiSchema';

const table = schemas as Record<string, { schema: unknown; usage: string | null }>;

describe('koishiSchema', () => {
    it('hydrates real server schema', () => {
        const node = hydrateSchema(table.server.schema);
        expect(node).not.toBeNull();
        const keys = objectFields(node!).map((f) => f.key);
        expect(keys).toEqual(expect.arrayContaining(['host', 'port', 'maxPort', 'selfUrl']));
        expect(renderable(node!)).toBe(true);
    });

    it('adapter-onebot is a tagged union on protocol', () => {
        const node = hydrateSchema(table['adapter-onebot'].schema)!;
        const sections = objectSections(node);
        const union = sections.flatMap((s) => s.fields).find((f) => f.key === '' && f.node.type === 'union');
        expect(union).toBeTruthy();
        const shape = unionShape(union!.node);
        expect(shape.kind).toBe('tagged');
        if (shape.kind !== 'tagged') return;
        expect(shape.key).toBe('protocol');
        expect(shape.branches.map((b) => b.value)).toEqual(expect.arrayContaining(['http', 'ws', 'ws-reverse']));
        // 没写 protocol 时落在 required(false) 的 ws-reverse 那支（上游默认值）
        expect(shape.branches[taggedBranch(shape, {})].value).toBe('ws-reverse');
        expect(shape.branches[taggedBranch(shape, { protocol: 'ws' })].value).toBe('ws');
    });

    it('selfId is required and reported when missing', () => {
        const node = hydrateSchema(table['adapter-onebot'].schema)!;
        expect(missingRequired(node, {})).toContain('selfId');
        expect(missingRequired(node, { selfId: '10001' })).not.toContain('selfId');
    });

    it('global schema hides computed $switch branches', () => {
        const node = hydrateSchema(table[''].schema)!;
        const prefix = objectFields(node).find((f) => f.key === 'prefix')!;
        expect(prefix).toBeTruthy();
        if (prefix.node.type === 'union') {
            expect(visibleBranches(prefix.node).every((b) => b.meta.hidden !== true)).toBe(true);
        }
        expect(renderable(node)).toBe(true);
        expect(objectSections(node).some((s) => s.title === '基础设置')).toBe(true);
    });

    it('enum unions become options', () => {
        const node = hydrateSchema(table[''].schema)!;
        const i18n = objectFields(node).find((f) => f.key === 'i18n')!.node;
        const output = objectFields(i18n).find((f) => f.key === 'output')!.node;
        const shape = unionShape(output);
        expect(shape.kind).toBe('enum');
        if (shape.kind === 'enum') expect(shape.options.map((o) => o.value)).toEqual(['prefer-user', 'prefer-channel']);
        expect(describeNode(output)).toBe('输出语言偏好设置。');
    });

    it('text helpers and blanks', () => {
        expect(schemaText({ 'zh-CN': '中', 'en-US': 'en' })).toBe('中');
        expect(schemaText({ '': 'x' })).toBe('x');
        expect(schemaText(3)).toBe('');
        expect(hydrateSchema({ nope: 1 })).toBeNull();
        expect(blankOf({ uid: 0, type: 'number', meta: { min: 5 } })).toBe(5);
        expect(blankOf({ uid: 0, type: 'boolean', meta: {} })).toBe(false);
        expect(blankOf({ uid: 0, type: 'string', meta: { default: 'a' } })).toBe('a');
    });
});
