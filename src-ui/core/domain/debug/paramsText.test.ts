import { describe, expect, it } from 'vitest';
import { formatParams, initialParamsText, parseParamsText, setParam } from './paramsText';
import { buildFormModel, coerceInput } from './schemaForm';

const NC_SCHEMA = {
    type: 'object',
    properties: {
        group_id: { type: 'string', 'x-ncd-role': 'group_id' },
        message: { anyOf: [{ type: 'array' }, { type: 'string' }], 'x-ncd-role': 'message' },
        auto_escape: { anyOf: [{ type: 'boolean' }, { type: 'string' }] },
    },
    required: ['group_id', 'message'],
};

const SL_SCHEMA = {
    type: 'object',
    properties: {
        group_id: { type: 'integer', minimum: 1, 'x-ncd-role': 'group_id' },
        user_id: { type: 'integer', 'x-ncd-role': 'member_id' },
        duration: { type: 'integer', default: 1800 },
        enable: { type: 'boolean' },
        tags: { type: 'array', items: { type: 'string' } },
        extra: { type: 'object' },
        message: { 'x-ncd-role': 'message' },
    },
    required: ['group_id', 'user_id', 'enable', 'tags', 'extra', 'message'],
};

describe('parseParamsText', () => {
    it('空串和纯空白是 {}', () => {
        expect(parseParamsText('')).toEqual({ ok: true, value: {} });
        expect(parseParamsText('  \n\t ')).toEqual({ ok: true, value: {} });
    });

    it('合法对象原样解析', () => {
        expect(parseParamsText('{"group_id":"1","n":2}')).toEqual({
            ok: true,
            value: { group_id: '1', n: 2 },
        });
    });

    it('不是对象的 JSON 报「参数必须是 JSON 对象」并指到第一个字符', () => {
        for (const text of ['[1,2]', '"x"', '42', 'null', 'true']) {
            const r = parseParamsText(text);
            expect(r).toMatchObject({
                ok: false,
                message: '参数必须是 JSON 对象',
                line: 1,
                column: 1,
            });
        }
        expect(parseParamsText('\n  [1]')).toMatchObject({ ok: false, line: 2, column: 3 });
    });

    it('语法错误给出行列：多余逗号', () => {
        const text = '{\n  "a": 1,\n}';
        expect(parseParamsText(text)).toMatchObject({ ok: false, line: 3, column: 1 });
    });

    it('语法错误给出行列：缺值（引擎报错本身不带位置的那一类）', () => {
        const text = '{\n  "a": 1,\n  "b": ,\n  "c": 2\n}';
        const r = parseParamsText(text);
        expect(r).toMatchObject({ ok: false, line: 3, column: 8 });
        expect(r.ok === false && r.message).toContain('意外的字符 ","');
    });

    it('语法错误给出行列：单引号属性名', () => {
        const r = parseParamsText("{'a': 1}");
        expect(r).toMatchObject({ ok: false, line: 1, column: 2 });
        expect(r.ok === false && r.message).toContain('双引号');
    });

    it('语法错误给出行列：缺冒号 / 缺逗号 / 尾部多余内容', () => {
        expect(parseParamsText('{"a" 1}')).toMatchObject({ ok: false, line: 1, column: 6 });
        expect(parseParamsText('{"a": 1 "b": 2}')).toMatchObject({ ok: false, line: 1, column: 9 });
        expect(parseParamsText('{"a": 1} x')).toMatchObject({ ok: false, line: 1, column: 10 });
    });

    it('没写完的 JSON 指到末尾并提示少了括号或引号', () => {
        const r = parseParamsText('{\n  "a": [1, 2');
        expect(r).toMatchObject({
            ok: false,
            message: 'JSON 不完整，可能少了括号或引号',
            line: 2,
            column: 13,
        });
        expect(parseParamsText('{"a": "abc')).toMatchObject({
            ok: false,
            message: 'JSON 不完整，可能少了括号或引号',
        });
    });

    it('字符串里直接换行也指到那一行', () => {
        const r = parseParamsText('{\n  "a": "x\ny"\n}');
        expect(r).toMatchObject({ ok: false, line: 2, column: 10 });
    });

    it('CRLF 换行的行号也对', () => {
        expect(parseParamsText('{\r\n  "a": 1,\r\n}')).toMatchObject({
            ok: false,
            line: 3,
            column: 1,
        });
    });
});

describe('formatParams', () => {
    it('两空格缩进，空对象是 {}', () => {
        expect(formatParams({})).toBe('{}');
        expect(formatParams({ a: 1, b: { c: [1] } })).toBe(
            '{\n  "a": 1,\n  "b": {\n    "c": [\n      1\n    ]\n  }\n}',
        );
    });
});

describe('setParam', () => {
    it('改已有键：留在原位，其余键不动', () => {
        const text = '{"a":1,"b":2,"c":3}';
        const next = setParam(text, 'b', 20);
        expect(Object.keys(JSON.parse(next))).toEqual(['a', 'b', 'c']);
        expect(JSON.parse(next)).toEqual({ a: 1, b: 20, c: 3 });
    });

    it('新键追加在末尾', () => {
        const next = setParam('{"a":1}', 'z', 'x');
        expect(Object.keys(JSON.parse(next))).toEqual(['a', 'z']);
    });

    it('undefined 删键', () => {
        const next = setParam('{"a":1,"b":2}', 'a', undefined);
        expect(JSON.parse(next)).toEqual({ b: 2 });
        expect(next).not.toContain('"a"');
    });

    it('删不存在的键：文本原样返回，不重排用户格式', () => {
        const text = '{ "a" :   1 }';
        expect(setParam(text, 'ghost', undefined)).toBe(text);
    });

    it('文本不合法时原样返回', () => {
        const bad = '{"a": ';
        expect(setParam(bad, 'a', 1)).toBe(bad);
        expect(setParam('[1]', 'a', 1)).toBe('[1]');
    });

    it('空文本从 {} 开始', () => {
        expect(JSON.parse(setParam('', 'a', 1))).toEqual({ a: 1 });
    });

    it('false / 0 / 空串是有效值，不会被当成删除', () => {
        const next = setParam('{}', 'flag', false);
        expect(JSON.parse(next)).toEqual({ flag: false });
        expect(JSON.parse(setParam(next, 'n', 0))).toEqual({ flag: false, n: 0 });
        expect(JSON.parse(setParam('{}', 's', ''))).toEqual({ s: '' });
    });

    it('__proto__ 一类的键名只当普通键，不污染原型', () => {
        const next = setParam('{}', '__proto__', { polluted: true });
        expect(({} as Record<string, unknown>).polluted).toBeUndefined();
        const parsed = JSON.parse(next) as Record<string, unknown>;
        expect(Object.prototype.hasOwnProperty.call(parsed, '__proto__')).toBe(true);
    });
});

describe('表单 ⇄ JSON 往返', () => {
    const model = buildFormModel(SL_SCHEMA);
    const fieldOf = (name: string) => model.fields.find((f) => f.name === name)!;

    it('表单改字段：文本里表单不认识的键保留，顺序不变', () => {
        // 用户在 JSON 里先手写了一个 schema 之外的键，并且把 user_id 放在 group_id 前面
        let text = '{\n  "user_id": 5,\n  "vendor_flag": {"deep": [1, 2]},\n  "group_id": 100\n}';
        text = setParam(text, 'group_id', coerceInput(fieldOf('group_id'), '200'));
        text = setParam(text, 'duration', coerceInput(fieldOf('duration'), '60'));
        expect(JSON.parse(text)).toEqual({
            user_id: 5,
            vendor_flag: { deep: [1, 2] },
            group_id: 200,
            duration: 60,
        });
        expect(Object.keys(JSON.parse(text))).toEqual([
            'user_id',
            'vendor_flag',
            'group_id',
            'duration',
        ]);
    });

    it('表单清空字段（空串）就是删键，其它键不受影响', () => {
        let text = '{"group_id":1,"user_id":2,"vendor_flag":true}';
        text = setParam(text, 'user_id', coerceInput(fieldOf('user_id'), ''));
        expect(JSON.parse(text)).toEqual({ group_id: 1, vendor_flag: true });
        expect(Object.keys(JSON.parse(text))).toEqual(['group_id', 'vendor_flag']);
    });

    it('各类控件写进去的值类型正确', () => {
        let text = '{}';
        text = setParam(text, 'enable', coerceInput(fieldOf('enable'), 'true'));
        text = setParam(text, 'tags', coerceInput(fieldOf('tags'), 'a, b'));
        text = setParam(text, 'extra', coerceInput(fieldOf('extra'), '{"k":1}'));
        expect(JSON.parse(text)).toEqual({ enable: true, tags: ['a', 'b'], extra: { k: 1 } });
    });

    it('反过来：文本改动后重新解析，得到的值就是表单该显示的值', () => {
        const parsed = parseParamsText('{"group_id": 7, "junk": null}');
        expect(parsed).toEqual({ ok: true, value: { group_id: 7, junk: null } });
    });
});

describe('initialParamsText', () => {
    it('spec 为空是 {}', () => {
        expect(initialParamsText(null)).toBe('{}');
    });

    it('有示例就用第一个示例', () => {
        const text = initialParamsText({
            examples: [{ group_id: '123456', message: 'hello' }, { group_id: '2' }],
            params_schema: NC_SCHEMA,
        });
        expect(JSON.parse(text)).toEqual({ group_id: '123456', message: 'hello' });
    });

    it('示例不是对象时跳过，用后面的对象示例', () => {
        const text = initialParamsText({
            examples: ['oops', { group_id: '9' }],
            params_schema: NC_SCHEMA,
        });
        expect(JSON.parse(text)).toEqual({ group_id: '9' });
    });

    it('没有示例：必填项带空值占位（NapCat 字符串 id）', () => {
        const text = initialParamsText({ examples: [], params_schema: NC_SCHEMA });
        expect(JSON.parse(text)).toEqual({ group_id: '', message: '' });
    });

    it('没有示例：必填项按类型给空值（布尔、数组、对象）；整数 id 不放 0 占位，空着让校验标「必填」', () => {
        const text = initialParamsText({ examples: [], params_schema: SL_SCHEMA });
        expect(JSON.parse(text)).toEqual({ enable: false, tags: [], extra: {}, message: '' });
    });

    it('必填项只有整数 id（撤回、查消息这类）时是 {}，不会出现 0', () => {
        const schema = {
            type: 'object',
            properties: { message_id: { type: 'integer', 'x-ncd-role': 'message_id' } },
            required: ['message_id'],
        };
        expect(initialParamsText({ examples: [], params_schema: schema })).toBe('{}');
    });

    it('没有必填项就是 {}', () => {
        expect(
            initialParamsText({
                examples: [],
                params_schema: { type: 'object', properties: { a: { type: 'string' } } },
            }),
        ).toBe('{}');
        expect(initialParamsText({ examples: [], params_schema: {} })).toBe('{}');
    });
});
