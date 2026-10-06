import { describe, expect, it } from 'vitest';
import type { DebugChannels } from '../../../core/ipc/generated/debug/DebugChannels';
import {
    baseActionName,
    countTreeRows,
    elapsedText,
    isBlankParams,
    isIdValue,
    isImageUrl,
    lineOfKey,
    localInputToSeconds,
    paramsDirty,
    sameJson,
    schemaTypeText,
    secondsToLocalInput,
    segmentsToText,
    sendBlocker,
    simplifySchema,
    textToSegments,
    valueText,
    type SendState,
} from './viewHelpers';

describe('参数文本', () => {
    it('空文本和 {} 算没填；改过 = 不空且不是初始那份', () => {
        expect(isBlankParams('  ')).toBe(true);
        expect(isBlankParams(' {} ')).toBe(true);
        expect(isBlankParams('{"a":1}')).toBe(false);
        expect(paramsDirty('{}', '{"a":0}')).toBe(false);
        expect(paramsDirty('{"a":0}', '{"a":0}')).toBe(false);
        expect(paramsDirty('{"a":1}', '{"a":0}')).toBe(true);
    });

    it('sameJson 按内容比，undefined 只等于 undefined', () => {
        expect(sameJson({ a: [1] }, { a: [1] })).toBe(true);
        expect(sameJson(1, '1')).toBe(false);
        expect(sameJson(undefined, null)).toBe(false);
        expect(sameJson(undefined, undefined)).toBe(true);
    });

    it('valueText 把值写成输入框里的样子', () => {
        expect(valueText(undefined)).toBe('');
        expect(valueText(null)).toBe('');
        expect(valueText(12)).toBe('12');
        expect(valueText(false)).toBe('false');
        expect(valueText({ a: 1 })).toBe('{"a":1}');
    });

    it('lineOfKey 找到顶层键所在的行，值里出现同名字符串不算', () => {
        const text = '{\n  "note": "group_id",\n  "group_id": 1\n}';
        expect(lineOfKey(text, 'group_id')).toBe(3);
        expect(lineOfKey(text, 'missing')).toBeNull();
    });
});

describe('接口名', () => {
    it('_async / _rate_limited 变体对应回原接口', () => {
        expect(baseActionName('send_group_msg_async')).toBe('send_group_msg');
        expect(baseActionName('send_msg_rate_limited')).toBe('send_msg');
        expect(baseActionName(' get_status ')).toBe('get_status');
        expect(baseActionName('async_thing')).toBe('async_thing');
    });
});

describe('回包里的值', () => {
    it('认 QQ 图床和图片扩展名的 http 地址', () => {
        expect(isImageUrl('https://multimedia.nt.qq.com.cn/download?appid=1407&rkey=x')).toBe(true);
        expect(isImageUrl('http://gchat.qpic.cn/gchatpic_new/1/2-3/0')).toBe(true);
        expect(isImageUrl('https://example.com/a.PNG?x=1')).toBe(true);
        expect(isImageUrl('https://example.com/a.txt')).toBe(false);
        expect(isImageUrl('file:///C:/a.png')).toBe(false);
        expect(isImageUrl(42)).toBe(false);
    });

    it('id 值是数字或纯数字串（允许负号）', () => {
        expect(isIdValue(100001)).toBe(true);
        expect(isIdValue('100001')).toBe(true);
        expect(isIdValue('-12')).toBe(true);
        expect(isIdValue('abc')).toBe(false);
        expect(isIdValue(Number.NaN)).toBe(false);
    });
});

describe('schema 展示', () => {
    it('类型写法', () => {
        expect(schemaTypeText({ type: 'string' })).toBe('string');
        expect(schemaTypeText({ type: 'array', items: { type: 'integer' } })).toBe('integer[]');
        expect(schemaTypeText({ anyOf: [{ type: 'boolean' }, { type: 'string' }] })).toBe(
            'boolean | string',
        );
        expect(schemaTypeText({ anyOf: [{ const: 'a' }, { const: 'b' }] })).toBe('enum');
        expect(schemaTypeText(undefined)).toBe('any');
    });

    it('返回结构压成「键 → 类型 · 说明」的树', () => {
        const schema = {
            type: 'object',
            properties: {
                user_id: { type: 'integer', description: 'QQ 号' },
                tags: {
                    type: 'array',
                    items: { type: 'object', properties: { name: { type: 'string' } } },
                },
                extra: {
                    anyOf: [
                        { type: 'object', properties: { x: { type: 'number' } } },
                        { type: 'null' },
                    ],
                },
            },
        };
        expect(simplifySchema(schema)).toEqual({
            user_id: 'integer · QQ 号',
            tags: [{ name: 'string' }],
            extra: { x: 'number' },
        });
        expect(countTreeRows(simplifySchema(schema))).toBe(7);
    });
});

describe('消息段', () => {
    it('文本 ⇄ 消息段；有非文本段时换不回文本', () => {
        expect(textToSegments('hi')).toEqual([{ type: 'text', data: { text: 'hi' } }]);
        expect(textToSegments('')).toEqual([]);
        expect(
            segmentsToText([
                { type: 'text', data: { text: 'a' } },
                { type: 'text', data: { text: 'b' } },
            ]),
        ).toBe('ab');
        expect(segmentsToText([{ type: 'face', data: { id: '1' } }])).toBeNull();
        expect(segmentsToText('x')).toBeNull();
    });
});

describe('时间戳', () => {
    it('Unix 秒和 datetime-local 文本来回转换', () => {
        const seconds = localInputToSeconds('2026-09-29T14:02:05');
        expect(seconds).not.toBeNull();
        expect(secondsToLocalInput(seconds)).toBe('2026-09-29T14:02:05');
        expect(secondsToLocalInput(String(seconds))).toBe('2026-09-29T14:02:05');
        expect(secondsToLocalInput('abc')).toBe('');
        expect(localInputToSeconds('')).toBeNull();
    });
});

describe('发送按钮', () => {
    const channels: DebugChannels = {
        bot_id: 'b',
        channels: [
            {
                id: { kind: 'internal' },
                label: '内部通道',
                can_call: true,
                can_receive: true,
                status: { kind: 'available' },
                endpoint: null,
                token_hint: null,
            },
        ],
        auto_call: { kind: 'internal' },
        auto_events: { kind: 'internal' },
    };
    const ok: SendState = {
        hasTarget: true,
        running: true,
        action: 'get_status',
        stream: false,
        localFileCount: 0,
        parseOk: true,
        specLoading: false,
        channels,
        channel: { kind: 'auto' },
    };

    it('按「最先该解决的」给原因，全好时是 null', () => {
        expect(sendBlocker(ok)).toBeNull();
        expect(sendBlocker({ ...ok, hasTarget: false })).toBe('先在顶栏选一个 Bot');
        expect(sendBlocker({ ...ok, action: ' ' })).toBe('先填接口名');
        expect(sendBlocker({ ...ok, running: false, parseOk: false })).toBe('Bot 没在运行');
        expect(sendBlocker({ ...ok, parseOk: false })).toBe('JSON 有错，改好再发');
        expect(sendBlocker({ ...ok, specLoading: true })).toBe('正在读取接口说明…');
        expect(sendBlocker({ ...ok, channels: { ...channels, auto_call: null } })).toBe(
            '没有能用的调用通道',
        );
        expect(sendBlocker({ ...ok, channel: { kind: 'http', name: 'gone' } })).toMatch(/不在了/);
        // 通道列表还没读到时不挡：让后端去解析
        expect(
            sendBlocker({ ...ok, channels: undefined, channel: { kind: 'http', name: 'gone' } }),
        ).toBeNull();
    });

    it('流式接口现在能发：分块下载在点名的 HTTP / WS 通道上才挡，内部通道和「自动」放行', () => {
        const withHttp: DebugChannels = {
            ...channels,
            channels: [
                ...channels.channels,
                {
                    id: { kind: 'http', name: 'h' },
                    label: 'HTTP · h',
                    can_call: true,
                    can_receive: false,
                    status: { kind: 'available' },
                    endpoint: null,
                    token_hint: null,
                },
            ],
        };
        const http = { kind: 'http', name: 'h' } as const;
        const download = {
            ...ok,
            stream: true,
            action: 'download_file_stream',
            channels: withHttp,
        };
        expect(sendBlocker(download)).toBeNull();
        expect(sendBlocker({ ...download, channel: { kind: 'internal' } })).toBeNull();
        expect(sendBlocker({ ...download, channel: http })).toMatch(/不支持流式/);
        expect(sendBlocker({ ...download, channel: { kind: 'ws', name: 'w' } })).toMatch(
            /不支持流式/,
        );

        const upload = { ...download, action: 'upload_file_stream' };
        // 手填块调用在哪条通道都行；带本机文件才需要内部通道
        expect(sendBlocker({ ...upload, channel: http })).toBeNull();
        expect(sendBlocker({ ...upload, localFileCount: 1, channel: http })).toMatch(/不支持流式/);
        // clean_stream_temp_file 是普通的单帧调用，任何通道都能发
        expect(
            sendBlocker({ ...download, action: 'clean_stream_temp_file', channel: http }),
        ).toBeNull();
        // 别的动作带本机文件：传输走内部通道，调用照样走所选通道，不挡
        expect(
            sendBlocker({ ...ok, localFileCount: 1, channel: http, channels: withHttp }),
        ).toBeNull();
    });

    it('等待时间的写法', () => {
        expect(elapsedText(1234)).toBe('1.2 秒');
        expect(elapsedText(61_500)).toBe('1 分 1 秒');
    });
});
