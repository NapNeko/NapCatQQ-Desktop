import { describe, expect, it } from 'vitest';
import {
    BOT_SCHEMA,
    MODEL_SCHEMA,
    fieldOf,
    getIn,
    newItemFor,
    nodeAt,
    schemaIssues,
    setIn,
} from './index';

describe('nodeAt / fieldOf', () => {
    it('沿真实路径取到嵌套节点', () => {
        const replyTiming = nodeAt(BOT_SCHEMA, ['chat', 'reply_timing']);
        expect(replyTiming).toBeDefined();
        const talkValue = fieldOf(replyTiming, 'talk_value');
        expect(talkValue?.type).toBe('number');
        expect(talkValue?.minValue).toBe(0);
        expect(talkValue?.maxValue).toBe(1);
    });

    it('路径不存在时返回 undefined 而不是抛', () => {
        expect(nodeAt(BOT_SCHEMA, ['chat', 'nope'])).toBeUndefined();
        expect(nodeAt(BOT_SCHEMA, ['nope', 'deeper'])).toBeUndefined();
        expect(fieldOf(nodeAt(BOT_SCHEMA, ['chat']), 'nope')).toBeUndefined();
        expect(fieldOf(undefined, 'talk_value')).toBeUndefined();
    });

    it('model.json 顶层有 models / model_task_config / api_providers', () => {
        for (const key of ['models', 'model_task_config', 'api_providers']) {
            expect(nodeAt(MODEL_SCHEMA, [key])).toBeDefined();
        }
        expect(fieldOf(nodeAt(MODEL_SCHEMA, ['models']), 'model_identifier')).toBeDefined();
    });
});

describe('getIn', () => {
    it('逐层取值', () => {
        const obj = { a: { b: { c: 42 } } };
        expect(getIn(obj, ['a', 'b', 'c'])).toBe(42);
        expect(getIn(obj, [])).toBe(obj);
    });

    it('中途遇到标量或 null 返回 undefined', () => {
        expect(getIn({ a: 1 }, ['a', 'b'])).toBeUndefined();
        expect(getIn({ a: null }, ['a', 'b'])).toBeUndefined();
        expect(getIn(undefined, ['a'])).toBeUndefined();
    });
});

describe('setIn', () => {
    it('沿途每层换新对象，兄弟引用不动', () => {
        const src = { a: { x: 1, y: { deep: 2 } }, b: { z: 3 } };
        const next = setIn(src, ['a', 'y', 'deep'], 99);
        expect(next).toEqual({ a: { x: 1, y: { deep: 99 } }, b: { z: 3 } });
        expect(next).not.toBe(src);
        expect(next.a).not.toBe(src.a);
        expect(next.a.y).not.toBe(src.a.y);
        expect(next.b).toBe(src.b);
        expect(src.a.y.deep).toBe(2);
    });

    it('空路径直接返回新值', () => {
        expect(setIn({ a: 1 }, [], 'replaced')).toBe('replaced');
    });

    it('源缺层时补建对象；null 源也可写', () => {
        expect(setIn({}, ['a', 'b'], 1)).toEqual({ a: { b: 1 } });
        expect(setIn<{ a: number }>(null as never, ['a'], 1)).toEqual({ a: 1 });
    });

    it('写 schema 默认值后 getIn 读回同值（往返）', () => {
        const empty = newItemFor(nodeAt(BOT_SCHEMA, ['chat', 'reply_timing'])!);
        const written = setIn(empty, ['reply_timing', 'talk_value'], 0.42);
        expect(getIn(written, ['reply_timing', 'talk_value'])).toBe(0.42);
    });
});

describe('schemaIssues', () => {
    const replyTimingNode = nodeAt(BOT_SCHEMA, ['chat', 'reply_timing'])!;

    it('闭区间内不报，越界报 range 措辞', () => {
        expect(schemaIssues(replyTimingNode, { talk_value: 0.5 }, '/chat/reply_timing')).toEqual(
            [],
        );
        expect(schemaIssues(replyTimingNode, { talk_value: 1.2 }, '/chat/reply_timing')).toEqual([
            { path: '/chat/reply_timing/talk_value', message: '要在 0 到 1 之间' },
        ]);
        expect(schemaIssues(replyTimingNode, { talk_value: -0.1 }, '/chat/reply_timing')).toEqual([
            { path: '/chat/reply_timing/talk_value', message: '要在 0 到 1 之间' },
        ]);
    });

    it('只有下界的字段报「不能小于」', () => {
        expect(
            schemaIssues(replyTimingNode, { max_consecutive_wait_count: 0 }, '/chat/reply_timing'),
        ).toEqual([
            {
                path: '/chat/reply_timing/max_consecutive_wait_count',
                message: '不能小于 1',
            },
        ]);
    });

    it('select 值不在 options 里报 one_of，合法值不报', () => {
        const experimental = nodeAt(BOT_SCHEMA, ['experimental'])!;
        expect(schemaIssues(experimental, { emotion_trait: 'neutral' }, '/experimental')).toEqual(
            [],
        );
        expect(schemaIssues(experimental, { emotion_trait: 'yandere' }, '/experimental')).toEqual([
            {
                path: '/experimental/emotion_trait',
                message: '只能是 rational_calm / neutral / sentimental 之一，现在是 "yandere"',
            },
        ]);
    });

    it('非 select 字段的字符串不拦（options 只是建议值）', () => {
        const synthetic = {
            fields: [{ name: 'platform', type: 'string' as const, options: ['onebot'] }],
        };
        expect(schemaIssues(synthetic, { platform: 'telegram' }, '/x')).toEqual([]);
    });

    it('数组小节逐条下钻，路径带下标', () => {
        const issues = schemaIssues(
            replyTimingNode,
            {
                talk_value_rules: [{ rule_type: 'group' }, { rule_type: 'everyone' }],
            },
            '/chat/reply_timing',
        );
        expect(issues).toEqual([
            {
                path: '/chat/reply_timing/talk_value_rules/1/rule_type',
                message: '只能是 group / private 之一，现在是 "everyone"',
            },
        ]);
    });

    it('非对象值直接跳过，不抛', () => {
        expect(schemaIssues(replyTimingNode, null, '/x')).toEqual([]);
        expect(schemaIssues(replyTimingNode, 'str', '/x')).toEqual([]);
    });
});

describe('newItemFor', () => {
    it('有默认值用默认值', () => {
        const item = newItemFor(nodeAt(BOT_SCHEMA, ['chat', 'reply_timing'])!);
        expect(item.talk_value).toBe(1);
        expect(item.reply_trigger_mode).toBe('frequency');
    });

    it('嵌套小节与嵌套数组元素递归展开', () => {
        const rule = newItemFor(nodeAt(BOT_SCHEMA, ['chat', 'reply_timing', 'talk_value_rules'])!);
        expect(rule).toMatchObject({ platform: '', item_id: '', rule_type: 'group', value: 0.5 });
    });

    it('容器默认值深拷贝，两次生成互不共享引用', () => {
        const webui = nodeAt(BOT_SCHEMA, ['webui'])!;
        const first = newItemFor(webui);
        const second = newItemFor(webui);
        expect(first.host).toEqual(['127.0.0.1', '::1']);
        expect(first.host).not.toBe(second.host);
        (first.host as string[]).push('0.0.0.0');
        expect(second.host).toEqual(['127.0.0.1', '::1']);
    });

    it('array 字段无默认值时给空数组', () => {
        const item = newItemFor(nodeAt(BOT_SCHEMA, ['chat', 'reply_timing'])!);
        expect(item.talk_value_rules).toEqual([]);
    });
});

describe('schema 快照可遍历性', () => {
    it('bot 根节点声明了 fields 且每个字段有名有类型', () => {
        expect(BOT_SCHEMA.fields.length).toBeGreaterThan(0);
        for (const f of BOT_SCHEMA.fields) {
            expect(typeof f.name).toBe('string');
            expect(f.name).toBeTruthy();
            expect(typeof f.type).toBe('string');
        }
    });
});
