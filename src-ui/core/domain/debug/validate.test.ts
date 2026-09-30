import { describe, expect, it } from 'vitest';
import { validateParams } from './validate';

const NC_SCHEMA = {
    type: 'object',
    properties: {
        group_id: { type: 'string' },
        message: { anyOf: [{ type: 'array' }, { type: 'string' }] },
        auto_escape: { anyOf: [{ type: 'boolean' }, { type: 'string' }] },
    },
    required: ['group_id', 'message'],
};

const SL_SCHEMA = {
    type: 'object',
    properties: {
        group_id: { type: 'integer', minimum: 1 },
        user_id: { type: 'integer' },
        duration: { type: 'integer', minimum: 0, maximum: 2592000 },
        message_type: { type: 'string', enum: ['private', 'group'] },
    },
    required: ['group_id', 'user_id'],
    additionalProperties: false,
};

describe('validateParams', () => {
    it('没有 schema 就没有问题', () => {
        expect(validateParams(null, { a: 1 })).toEqual([]);
        expect(validateParams(undefined, {})).toEqual([]);
    });

    it('必填缺失', () => {
        expect(validateParams(NC_SCHEMA, {})).toEqual([
            { path: 'group_id', message: '必填' },
            { path: 'message', message: '必填' },
        ]);
    });

    it('必填项填了 null 或空串同样算漏填，不叠加类型错误', () => {
        expect(validateParams(NC_SCHEMA, { group_id: '', message: null })).toEqual([
            { path: 'group_id', message: '必填' },
            { path: 'message', message: '必填' },
        ]);
    });

    it('通过时返回空数组', () => {
        expect(validateParams(NC_SCHEMA, { group_id: '123', message: 'hi' })).toEqual([]);
        expect(validateParams(SL_SCHEMA, { group_id: 1, user_id: 2 })).toEqual([]);
    });

    it('类型错误：整数', () => {
        expect(validateParams(SL_SCHEMA, { group_id: '123', user_id: 2 })).toEqual([{ path: 'group_id', message: '应为整数' }]);
        expect(validateParams(SL_SCHEMA, { group_id: 1.5, user_id: 2 })).toEqual([{ path: 'group_id', message: '应为整数' }]);
    });

    it('其它类型的说法', () => {
        const schema = { type: 'object', properties: { a: { type: 'string' }, b: { type: 'boolean' }, c: { type: 'array' }, d: { type: 'object' }, e: { type: 'number' } } };
        expect(validateParams(schema, { a: 1, b: 'x', c: {}, d: [], e: 'n' })).toEqual([
            { path: 'a', message: '应为字符串' },
            { path: 'b', message: '应为布尔值' },
            { path: 'c', message: '应为数组' },
            { path: 'd', message: '应为对象' },
            { path: 'e', message: '应为数字' },
        ]);
    });

    it('number 接受整数；type 数组里任一命中即可', () => {
        const schema = { type: 'object', properties: { n: { type: 'number' }, s: { type: ['string', 'integer'] } } };
        expect(validateParams(schema, { n: 3, s: 4 })).toEqual([]);
        expect(validateParams(schema, { n: 3, s: 'x' })).toEqual([]);
        expect(validateParams(schema, { s: true })).toEqual([{ path: 's', message: '应为 字符串 / 整数 之一' }]);
    });

    it('anyOf：任一分支通过就行（NapCat 的布尔既可以是 boolean 也可以是字符串）', () => {
        expect(validateParams(NC_SCHEMA, { group_id: '1', message: 'x', auto_escape: true })).toEqual([]);
        expect(validateParams(NC_SCHEMA, { group_id: '1', message: 'x', auto_escape: 'false' })).toEqual([]);
        expect(validateParams(NC_SCHEMA, { group_id: '1', message: [] })).toEqual([]);
    });

    it('anyOf 全部不通过：报类型不符，列出可接受的类型', () => {
        expect(validateParams(NC_SCHEMA, { group_id: '1', message: 'x', auto_escape: 5 })).toEqual([
            { path: 'auto_escape', message: '应为 布尔值 / 字符串 之一' },
        ]);
    });

    it('anyOf 里类型对得上的分支才给具体原因（数字越界）', () => {
        const schema = {
            type: 'object',
            properties: { n: { anyOf: [{ type: 'integer', minimum: 10 }, { type: 'string' }] } },
        };
        expect(validateParams(schema, { n: 3 })).toEqual([{ path: 'n', message: '不能小于 10' }]);
        expect(validateParams(schema, { n: 30 })).toEqual([]);
    });

    it('anyOf of const：报可选值', () => {
        const schema = { type: 'object', properties: { t: { anyOf: [{ const: 'a' }, { const: 'b' }] } } };
        expect(validateParams(schema, { t: 'a' })).toEqual([]);
        expect(validateParams(schema, { t: 'c' })).toEqual([{ path: 't', message: '应为 a / b 之一' }]);
    });

    it('enum', () => {
        expect(validateParams(SL_SCHEMA, { group_id: 1, user_id: 2, message_type: 'group' })).toEqual([]);
        expect(validateParams(SL_SCHEMA, { group_id: 1, user_id: 2, message_type: 'guild' })).toEqual([
            { path: 'message_type', message: '应为 private / group 之一' },
        ]);
    });

    it('const', () => {
        const schema = { type: 'object', properties: { v: { const: 1 } } };
        expect(validateParams(schema, { v: 1 })).toEqual([]);
        expect(validateParams(schema, { v: 2 })).toEqual([{ path: 'v', message: '应为 1' }]);
    });

    it('枚举太长时只列前几个', () => {
        const values = Array.from({ length: 12 }, (_, i) => `v${i}`);
        const schema = { type: 'object', properties: { v: { enum: values } } };
        const [issue] = validateParams(schema, { v: 'zzz' });
        expect(issue?.message).toBe('应为 v0 / v1 / v2 / v3 / v4 / v5 / v6 / v7 … 之一');
    });

    it('minimum / maximum', () => {
        expect(validateParams(SL_SCHEMA, { group_id: 0, user_id: 2 })).toEqual([{ path: 'group_id', message: '不能小于 1' }]);
        expect(validateParams(SL_SCHEMA, { group_id: 1, user_id: 2, duration: 99999999 })).toEqual([
            { path: 'duration', message: '不能大于 2592000' },
        ]);
        expect(validateParams(SL_SCHEMA, { group_id: 1, user_id: 2, duration: 0 })).toEqual([]);
    });

    it('additionalProperties:false 拒绝多余的键；不写则放行', () => {
        expect(validateParams(SL_SCHEMA, { group_id: 1, user_id: 2, bogus: 1 })).toEqual([
            { path: 'bogus', message: '不支持这个参数' },
        ]);
        expect(validateParams(NC_SCHEMA, { group_id: '1', message: 'x', bogus: 1 })).toEqual([]);
    });

    it('additionalProperties:false 时，原型上的名字（constructor 等）也当作未声明的多余键', () => {
        const value = JSON.parse('{"group_id":1,"user_id":2,"constructor":1,"toString":2,"__proto__":3}') as Record<string, unknown>;
        expect(validateParams(SL_SCHEMA, value)).toEqual([
            { path: 'constructor', message: '不支持这个参数' },
            { path: 'toString', message: '不支持这个参数' },
            { path: '__proto__', message: '不支持这个参数' },
        ]);
        // schema 真的声明了同名参数时照常校验
        const declared = { type: 'object', properties: { constructor: { type: 'integer' } }, additionalProperties: false };
        expect(validateParams(declared, { constructor: 1 })).toEqual([]);
        expect(validateParams(declared, { constructor: 'x' })).toEqual([{ path: 'constructor', message: '应为整数' }]);
    });

    it('负数整数通过没有下限的整数 schema，违反 minimum 时才报错', () => {
        const schema = { type: 'object', properties: { message_id: { type: 'integer' }, n: { type: 'integer', minimum: 0 } } };
        expect(validateParams(schema, { message_id: -5 })).toEqual([]);
        expect(validateParams(schema, { n: -1 })).toEqual([{ path: 'n', message: '不能小于 0' }]);
    });

    it('items 校验数组元素，路径带下标；只查一层', () => {
        const schema = {
            type: 'object',
            properties: {
                ids: { type: 'array', items: { type: 'integer' } },
                news: { type: 'array', items: { type: 'object', properties: { text: { type: 'string' } }, required: ['text'] } },
                grid: { type: 'array', items: { type: 'array', items: { type: 'integer' } } },
            },
        };
        expect(validateParams(schema, { ids: [1, 'x', 3] })).toEqual([{ path: 'ids[1]', message: '应为整数' }]);
        expect(validateParams(schema, { news: [{ text: 'a' }, {}] })).toEqual([{ path: 'news[1].text', message: '必填' }]);
        // 元素本身是数组时，只检查它是不是数组，不再往里查
        expect(validateParams(schema, { grid: [[1, 'x']] })).toEqual([]);
        expect(validateParams(schema, { grid: [5] })).toEqual([{ path: 'grid[0]', message: '应为数组' }]);
    });

    it('嵌套对象里的必填也查', () => {
        const schema = {
            type: 'object',
            properties: { opt: { type: 'object', properties: { k: { type: 'integer' } }, required: ['k'] } },
        };
        expect(validateParams(schema, { opt: {} })).toEqual([{ path: 'opt.k', message: '必填' }]);
        expect(validateParams(schema, { opt: { k: 'x' } })).toEqual([{ path: 'opt.k', message: '应为整数' }]);
    });

    it('可选字段没填不报错', () => {
        expect(validateParams(SL_SCHEMA, { group_id: 1, user_id: 2 })).toEqual([]);
    });

    it('schema 里的未知关键字被忽略', () => {
        const schema = { type: 'object', properties: { a: { type: 'string', pattern: '^x', 'x-ncd-role': 'file', format: 'uri' } } };
        expect(validateParams(schema, { a: 'no' })).toEqual([]);
    });
});
