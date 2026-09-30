import { describe, expect, it } from 'vitest';
import { buildFormModel, coerceInput, type FormField } from './schemaForm';
import { validateParams } from './validate';

// NapCat 的 send_group_msg：TypeBox 生成，id 是字符串，布尔是 anyOf[boolean, string]，message 带 $id。
// x-ncd-role 是后端转换器统一补上的。
const NC_SEND_GROUP_MSG = {
    type: 'object',
    properties: {
        message_type: { description: '消息类型', enum: ['private', 'group'], type: 'string' },
        user_id: { description: '用户QQ', type: 'string', 'x-ncd-role': 'user_id' },
        group_id: { description: '群号', type: 'string', 'x-ncd-role': 'group_id' },
        message: {
            $id: 'OB11MessageMixType',
            anyOf: [{ type: 'array' }, { type: 'string' }],
            'x-ncd-role': 'message',
        },
        auto_escape: { description: '是否作为纯文本发送', anyOf: [{ type: 'boolean' }, { type: 'string' }] },
        source: { description: '合并转发来源', type: 'string' },
        news: {
            description: '合并转发外显',
            type: 'array',
            items: { type: 'object', properties: { text: { type: 'string' } }, required: ['text'] },
        },
    },
    required: ['message'],
} as const;

// SnowLuma：id 是整数并自带 role，布尔就是 boolean，message 没有 type
const SL_SEND_GROUP_MSG = {
    type: 'object',
    properties: {
        group_id: { type: 'integer', minimum: 1, description: '群号', 'x-role': 'group_id', 'x-ncd-role': 'group_id' },
        message: { description: 'OneBot message: string | segment[] | object', 'x-ncd-role': 'message' },
        auto_escape: { type: 'boolean', default: false },
    },
    required: ['group_id', 'message'],
    additionalProperties: true,
} as const;

const field = (schema: Record<string, unknown>, name: string): FormField => {
    const f = buildFormModel(schema).fields.find((x) => x.name === name);
    if (!f) throw new Error(`没有字段 ${name}`);
    return f;
};

const one = (prop: Record<string, unknown>, required = false): FormField =>
    field({ type: 'object', properties: { p: prop }, ...(required ? { required: ['p'] } : {}) }, 'p');

describe('buildFormModel · 整体结构', () => {
    it('null / undefined / 非对象 schema 给空表单，允许额外键', () => {
        expect(buildFormModel(null)).toEqual({ fields: [], allowsExtra: true });
        expect(buildFormModel(undefined)).toEqual({ fields: [], allowsExtra: true });
    });

    it('必填的排前面，同组内保持 schema 顺序', () => {
        const model = buildFormModel({
            type: 'object',
            properties: { a: { type: 'string' }, b: { type: 'string' }, c: { type: 'string' }, d: { type: 'string' } },
            required: ['d', 'b'],
        });
        expect(model.fields.map((f) => f.name)).toEqual(['b', 'd', 'a', 'c']);
        expect(model.fields.map((f) => f.required)).toEqual([true, true, false, false]);
    });

    it('required 点名但 properties 没定义的字段照样出现，当未知类型', () => {
        const model = buildFormModel({ type: 'object', properties: { a: { type: 'string' } }, required: ['ghost'] });
        expect(model.fields.map((f) => f.name)).toEqual(['ghost', 'a']);
        expect(model.fields[0]).toMatchObject({ required: true, kind: 'json', valueType: 'any' });
    });

    it('additionalProperties:false 才算不允许额外键', () => {
        expect(buildFormModel({ type: 'object', properties: {} }).allowsExtra).toBe(true);
        expect(buildFormModel({ type: 'object', properties: {}, additionalProperties: true }).allowsExtra).toBe(true);
        expect(buildFormModel({ type: 'object', properties: {}, additionalProperties: false }).allowsExtra).toBe(false);
    });
});

describe('buildFormModel · NapCat send_group_msg', () => {
    const model = buildFormModel(NC_SEND_GROUP_MSG);
    const by = (name: string) => model.fields.find((f) => f.name === name) as FormField;

    it('message 必填并排第一，其余保持 schema 顺序', () => {
        expect(model.fields.map((f) => f.name)).toEqual([
            'message',
            'message_type',
            'user_id',
            'group_id',
            'auto_escape',
            'source',
            'news',
        ]);
    });

    it('按 role 出控件：字符串 id 只接受字符串', () => {
        expect(by('group_id')).toMatchObject({ kind: 'group', valueType: 'string', acceptsString: true, acceptsNumber: false });
        expect(by('user_id')).toMatchObject({ kind: 'friend', valueType: 'string', acceptsNumber: false });
        expect(by('message')).toMatchObject({ kind: 'message', valueType: 'any', acceptsString: true, required: true });
    });

    it('enum 保留取值，标签就是值本身', () => {
        expect(by('message_type')).toMatchObject({ kind: 'enum', valueType: 'string' });
        expect(by('message_type').enumValues).toEqual([
            { value: 'private', label: 'private' },
            { value: 'group', label: 'group' },
        ]);
    });

    it('TypeBox 的 anyOf[boolean, string] 是开关，同时记下也接受字符串', () => {
        expect(by('auto_escape')).toMatchObject({ kind: 'boolean', valueType: 'boolean', acceptsString: true, acceptsNumber: false });
        expect(by('auto_escape').description).toBe('是否作为纯文本发送');
    });

    it('对象数组走 JSON 编辑器，普通字符串走文本', () => {
        expect(by('news').kind).toBe('json');
        expect(by('source').kind).toBe('text');
    });
});

describe('buildFormModel · SnowLuma send_group_msg', () => {
    const model = buildFormModel(SL_SEND_GROUP_MSG);
    const by = (name: string) => model.fields.find((f) => f.name === name) as FormField;

    it('整数 id 接受数字，带 minimum', () => {
        expect(by('group_id')).toMatchObject({
            kind: 'group',
            valueType: 'integer',
            acceptsNumber: true,
            acceptsString: false,
            minimum: 1,
            required: true,
        });
    });

    it('没有 type 的 message 靠 role 认出来', () => {
        expect(by('message')).toMatchObject({ kind: 'message', acceptsString: true, acceptsNumber: true });
    });

    it('布尔带默认值', () => {
        expect(by('auto_escape')).toMatchObject({ kind: 'boolean', defaultValue: false, acceptsString: false });
    });
});

describe('buildFormModel · 控件类型判定', () => {
    it('x-ncd-role 各映射', () => {
        const kindOf = (role: string, type = 'string') => one({ type, 'x-ncd-role': role }).kind;
        expect(kindOf('group_id')).toBe('group');
        expect(kindOf('user_id')).toBe('friend');
        expect(kindOf('member_id')).toBe('member');
        expect(kindOf('message_id')).toBe('message_id');
        expect(kindOf('message')).toBe('message');
        expect(kindOf('file')).toBe('file');
        expect(kindOf('image')).toBe('file');
        expect(kindOf('record')).toBe('file');
        expect(kindOf('video')).toBe('file');
        expect(kindOf('face_id')).toBe('face');
        expect(kindOf('timestamp', 'integer')).toBe('timestamp');
    });

    it('role 优先于枚举和类型', () => {
        expect(one({ type: 'string', enum: ['a', 'b'], 'x-ncd-role': 'user_id' }).kind).toBe('friend');
    });

    it('不认识的 role 忽略，按类型走', () => {
        expect(one({ type: 'integer', 'x-ncd-role': 'duration' }).kind).toBe('number');
    });

    it('role 只对能装标量的字段生效：数组类型的 user_id 不变成好友选择器', () => {
        expect(one({ type: 'array', items: { type: 'integer' }, 'x-ncd-role': 'user_id' }).kind).toBe('array');
    });

    it('anyOf 全是 const 的（TypeBox Union of Literal）是枚举，可带标题', () => {
        const f = one({ anyOf: [{ const: 0, title: '关闭' }, { const: 1 }, { const: 2 }] });
        expect(f).toMatchObject({ kind: 'enum', valueType: 'integer', acceptsNumber: true, acceptsString: false });
        expect(f.enumValues).toEqual([
            { value: 0, label: '关闭' },
            { value: 1, label: '1' },
            { value: 2, label: '2' },
        ]);
    });

    it('anyOf 里混了非常量分支就不是枚举', () => {
        expect(one({ anyOf: [{ const: 'a' }, { type: 'string' }] }).kind).toBe('text');
    });

    it('单个 const 是只有一项的枚举', () => {
        expect(one({ const: 'group' }).enumValues).toEqual([{ value: 'group', label: 'group' }]);
    });

    it('布尔枚举、混合枚举的 valueType', () => {
        expect(one({ enum: [true, false] }).valueType).toBe('boolean');
        expect(one({ enum: ['a', 1] }).valueType).toBe('any');
    });

    it('boolean 和 anyOf 里含 boolean（且没有 object）是开关', () => {
        expect(one({ type: 'boolean' }).kind).toBe('boolean');
        expect(one({ anyOf: [{ type: 'boolean' }, { type: 'string' }] }).kind).toBe('boolean');
        expect(one({ type: ['boolean', 'string'] }).kind).toBe('boolean');
        expect(one({ anyOf: [{ type: 'boolean' }, { type: 'object' }] }).kind).toBe('json');
    });

    it('integer / number 是数字，valueType 区分整数和小数', () => {
        expect(one({ type: 'integer' })).toMatchObject({ kind: 'number', valueType: 'integer' });
        expect(one({ type: 'number' })).toMatchObject({ kind: 'number', valueType: 'number' });
        expect(one({ anyOf: [{ type: 'number' }, { type: 'string' }] })).toMatchObject({
            kind: 'number',
            acceptsString: true,
            acceptsNumber: true,
        });
    });

    it('minimum / maximum / default / description 会带出来，也认 anyOf 分支里的', () => {
        const f = one({ type: 'integer', minimum: 0, maximum: 2592000, default: 1800, description: '禁言时长（秒）' });
        expect(f).toMatchObject({ minimum: 0, maximum: 2592000, defaultValue: 1800, description: '禁言时长（秒）' });
        const g = one({ anyOf: [{ type: 'integer', minimum: 3, description: '分支里的说明' }, { type: 'string' }] });
        expect(g).toMatchObject({ minimum: 3, description: '分支里的说明' });
    });

    it('标量数组是列表，元素类型区分文本和数字', () => {
        expect(one({ type: 'array', items: { type: 'string' } })).toMatchObject({ kind: 'array', itemKind: 'text' });
        expect(one({ type: 'array', items: { type: 'integer' } })).toMatchObject({ kind: 'array', itemKind: 'number' });
        expect(one({ anyOf: [{ type: 'array', items: { type: 'number' } }, { type: 'null' }] })).toMatchObject({
            kind: 'array',
            itemKind: 'number',
        });
    });

    it('对象数组、没写 items 的数组、对象、未知类型、anyOf 含 object 都走 JSON', () => {
        expect(one({ type: 'array', items: { type: 'object' } }).kind).toBe('json');
        expect(one({ type: 'array' }).kind).toBe('json');
        expect(one({ type: 'object', properties: { a: { type: 'string' } } }).kind).toBe('json');
        expect(one({}).kind).toBe('json');
        expect(one({ description: '啥类型都行' }).kind).toBe('json');
        expect(one({ anyOf: [{ type: 'object' }, { type: 'string' }] }).kind).toBe('json');
        expect(one({ anyOf: [{ type: 'array' }, { type: 'string' }] }).kind).toBe('json');
    });

    it('nullable 的字符串仍然是文本', () => {
        expect(one({ type: ['string', 'null'] }).kind).toBe('text');
        expect(one({ anyOf: [{ type: 'string' }, { type: 'null' }] }).kind).toBe('text');
    });

    it('allOf 的片段并进来', () => {
        expect(one({ allOf: [{ type: 'integer' }, { minimum: 5 }] })).toMatchObject({ kind: 'number', minimum: 5 });
    });
});

describe('buildFormModel · 初始空值', () => {
    it('按类型给空值：字符串 id 给空串，整数 id 给 0，布尔 false，数组 []，对象 {}', () => {
        expect(one({ type: 'string', 'x-ncd-role': 'group_id' }).emptyValue).toBe('');
        expect(one({ type: 'integer', 'x-ncd-role': 'group_id' }).emptyValue).toBe(0);
        expect(one({ type: 'boolean' }).emptyValue).toBe(false);
        expect(one({ type: 'number' }).emptyValue).toBe(0);
        expect(one({ type: 'array', items: { type: 'string' } }).emptyValue).toEqual([]);
        expect(one({ type: 'array', items: { type: 'object' } }).emptyValue).toEqual([]);
        expect(one({ type: 'object' }).emptyValue).toEqual({});
        expect(one({ type: 'string' }).emptyValue).toBe('');
    });

    it('有默认值用默认值，枚举取第一项', () => {
        expect(one({ type: 'integer', default: 30 }).emptyValue).toBe(30);
        expect(one({ enum: ['x', 'y'] }).emptyValue).toBe('x');
    });
});

describe('coerceInput', () => {
    const num = one({ type: 'integer' });

    it('空串表示不填', () => {
        expect(coerceInput(num, '')).toBeUndefined();
        expect(coerceInput(one({ type: 'string' }), '')).toBeUndefined();
    });

    it('数字字段：数字文本变数字，其余原样留字符串交给校验', () => {
        expect(coerceInput(num, '42')).toBe(42);
        expect(coerceInput(num, ' -3 ')).toBe(-3);
        expect(coerceInput(one({ type: 'number' }), '1.5')).toBe(1.5);
        expect(coerceInput(one({ type: 'number' }), '1e3')).toBe(1000);
        expect(coerceInput(num, 'abc')).toBe('abc');
        expect(coerceInput(num, '0x10')).toBe('0x10');
        expect(coerceInput(num, '1.5')).toBe(1.5);
    });

    it('数字字段同时收字符串时，超出安全整数范围的纯数字留字符串，不丢精度', () => {
        const f = one({ anyOf: [{ type: 'integer' }, { type: 'string' }] });
        expect(coerceInput(f, '9007199254740993')).toBe('9007199254740993');
        expect(coerceInput(f, '123')).toBe(123);
    });

    it('id 选择器：收数字且是纯数字就转数字，否则字符串', () => {
        const sl = one({ type: 'integer', 'x-ncd-role': 'group_id' });
        const nc = one({ type: 'string', 'x-ncd-role': 'group_id' });
        const both = one({ anyOf: [{ type: 'integer' }, { type: 'string' }], 'x-ncd-role': 'user_id' });
        expect(coerceInput(sl, '123456')).toBe(123456);
        expect(coerceInput(sl, 'abc')).toBe('abc');
        expect(coerceInput(nc, '123456')).toBe('123456');
        expect(coerceInput(both, '123456')).toBe(123456);
        expect(coerceInput(both, '12a')).toBe('12a');
        expect(coerceInput(both, '9007199254740993')).toBe('9007199254740993');
    });

    it('id 类控件认负号：SnowLuma 的 message_id 是有符号 32 位整数，会出现负数', () => {
        const sl = one({ type: 'integer', 'x-ncd-role': 'message_id' }, true);
        expect(coerceInput(sl, '-2147483648')).toBe(-2147483648);
        expect(coerceInput(sl, '-5')).toBe(-5);
        expect(coerceInput(sl, ' -5 ')).toBe(-5);
        // 只有一个负号、夹着负号、小数都不是整数文本，留字符串
        expect(coerceInput(sl, '-')).toBe('-');
        expect(coerceInput(sl, '5-3')).toBe('5-3');
        expect(coerceInput(sl, '-1.5')).toBe('-1.5');
        for (const role of ['group_id', 'user_id', 'member_id']) {
            expect(coerceInput(one({ type: 'integer', 'x-ncd-role': role }), '-7')).toBe(-7);
        }
        // 字符串 id（NapCat）仍然原样
        expect(coerceInput(one({ type: 'string', 'x-ncd-role': 'message_id' }), '-5')).toBe('-5');
        // 超出安全整数的负数同样不丢精度
        expect(coerceInput(one({ anyOf: [{ type: 'integer' }, { type: 'string' }], 'x-ncd-role': 'message_id' }), '-9007199254740993')).toBe(
            '-9007199254740993',
        );
    });

    it('负数 message_id 转出来的值能通过整数 schema 的校验', () => {
        const schema = { type: 'object', properties: { message_id: { type: 'integer', 'x-ncd-role': 'message_id' } }, required: ['message_id'] };
        const f = field(schema, 'message_id');
        const value = coerceInput(f, '-123456');
        expect(value).toBe(-123456);
        expect(validateParams(schema, { message_id: value })).toEqual([]);
    });

    it('message_id / 成员 / 表情 / 时间戳同样走数字优先', () => {
        expect(coerceInput(one({ type: 'integer', 'x-ncd-role': 'message_id' }), '77')).toBe(77);
        expect(coerceInput(one({ type: 'integer', 'x-ncd-role': 'member_id' }), '77')).toBe(77);
        expect(coerceInput(one({ type: 'integer', 'x-ncd-role': 'face_id' }), '14')).toBe(14);
        expect(coerceInput(one({ type: 'integer', 'x-ncd-role': 'timestamp' }), '1700000000')).toBe(1700000000);
    });

    it('文件和消息文本原样返回', () => {
        expect(coerceInput(one({ type: 'string', 'x-ncd-role': 'file' }), '123')).toBe('123');
        expect(coerceInput(one({ 'x-ncd-role': 'message' }), 'hello [CQ:face,id=1]')).toBe('hello [CQ:face,id=1]');
    });

    it('布尔与枚举还原成带类型的值', () => {
        const b = one({ type: 'boolean' });
        expect(coerceInput(b, 'true')).toBe(true);
        expect(coerceInput(b, 'FALSE')).toBe(false);
        expect(coerceInput(b, 'maybe')).toBe('maybe');
        const e = one({ enum: [0, 1, 2] });
        expect(coerceInput(e, '1')).toBe(1);
        expect(coerceInput(e, '9')).toBe('9');
        expect(coerceInput(one({ enum: ['private', 'group'] }), 'group')).toBe('group');
    });

    it('数组：JSON 数组文本或按换行 / 逗号分隔，元素按 itemKind 转', () => {
        const nums = one({ type: 'array', items: { type: 'integer' } });
        const strs = one({ type: 'array', items: { type: 'string' } });
        expect(coerceInput(nums, '1, 2\n3')).toEqual([1, 2, 3]);
        expect(coerceInput(nums, '[4,5]')).toEqual([4, 5]);
        expect(coerceInput(strs, 'a,b\n\nc')).toEqual(['a', 'b', 'c']);
        expect(coerceInput(strs, '["x"]')).toEqual(['x']);
    });

    it('JSON 字段：合法就解析，不合法原样留字符串', () => {
        const j = one({ type: 'object' });
        expect(coerceInput(j, '{"a":1}')).toEqual({ a: 1 });
        expect(coerceInput(j, '{oops')).toBe('{oops');
    });
});
