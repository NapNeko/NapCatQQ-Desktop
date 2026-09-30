import { describe, expect, it } from 'vitest';
import type { ChatItem } from './chat';
import { DEFAULT_CHAT_FILTER, filterItems, type ChatFilter } from './chatFilter';

let seq = 0;
const base = () => {
    seq += 1;
    return { key: `e${seq}`, seq, at: seq * 1000 };
};

const message = (over: Partial<Extract<ChatItem, { kind: 'message' }>> = {}): ChatItem => ({
    ...base(),
    kind: 'message',
    session: 'group:1',
    direction: 'in',
    senderId: 10001,
    senderName: '小明',
    segments: [{ type: 'text', data: { text: 'Hello World' } }],
    raw: {},
    ...over,
});
const notice = (session: string | undefined, text: string): ChatItem => ({
    ...base(),
    kind: 'notice',
    ...(session ? { session } : {}),
    text,
    raw: {},
});
const request = (over: Partial<Extract<ChatItem, { kind: 'request' }>> = {}): ChatItem => ({
    ...base(),
    kind: 'request',
    requestType: 'friend',
    userId: 555,
    comment: '我是阿强',
    flag: 'f',
    raw: {},
    ...over,
});
const callItem = (action: string, summary = ''): ChatItem => ({
    ...base(),
    kind: 'call',
    action,
    origin: 'editor',
    call: { action, requestId: null, ok: true },
    summary,
    raw: {},
});
const meta = (heartbeat: boolean, text: string): ChatItem => ({ ...base(), kind: 'meta', text, heartbeat, raw: {} });
const gap = (): ChatItem => ({ ...base(), kind: 'gap', fromMs: 0, toMs: 1 });
const dropped = (): ChatItem => ({ ...base(), kind: 'dropped', count: 3 });
const receiver = (): ChatItem => ({ ...base(), kind: 'receiver', state: { state: 'connected' } });

const filter = (over: Partial<ChatFilter> = {}): ChatFilter => ({ ...DEFAULT_CHAT_FILTER, ...over });
const kinds = (over: Partial<ChatFilter['kinds']>) => ({ ...DEFAULT_CHAT_FILTER.kinds, ...over });

describe('DEFAULT_CHAT_FILTER', () => {
    it('全部会话、全部种类都开，心跳默认藏', () => {
        expect(DEFAULT_CHAT_FILTER).toEqual({
            session: 'all',
            kinds: { message: true, notice: true, request: true, call: true, meta: true },
            showHeartbeat: false,
            text: '',
        });
    });
});

describe('filterItems', () => {
    const sample = (): ChatItem[] => [
        message(),
        notice('group:1', '10 加入了群'),
        request(),
        callItem('get_group_list', 'no_cache=true'),
        meta(false, '生命周期：连接建立'),
        meta(true, '心跳'),
        gap(),
        dropped(),
        receiver(),
    ];

    it('默认筛选：只去掉心跳，其余保留', () => {
        const items = sample();
        const out = filterItems(items, filter());
        expect(out).toHaveLength(items.length - 1);
        expect(out.some((i) => i.kind === 'meta' && i.heartbeat)).toBe(false);
    });

    it('打开 showHeartbeat 后没有任何条目被筛掉，直接沿用原数组', () => {
        const items = sample();
        expect(filterItems(items, filter({ showHeartbeat: true }))).toBe(items);
    });

    it('按种类关掉', () => {
        const items = sample();
        const only = (k: keyof ChatFilter['kinds']) =>
            filterItems(items, filter({ kinds: { message: false, notice: false, request: false, call: false, meta: false, [k]: true }, showHeartbeat: true }))
                .filter((i) => !['gap', 'dropped', 'receiver'].includes(i.kind))
                .map((i) => i.kind);
        expect(only('message')).toEqual(['message']);
        expect(only('notice')).toEqual(['notice']);
        expect(only('request')).toEqual(['request']);
        expect(only('call')).toEqual(['call']);
        expect(only('meta')).toEqual(['meta', 'meta']);
    });

    it('缺口 / 丢弃 / 接收器状态不管怎么筛都保留，包括搜索和会话筛选', () => {
        const items = sample();
        const out = filterItems(
            items,
            filter({ kinds: kinds({ message: false, notice: false, request: false, call: false, meta: false }), session: 'group:9', text: '根本搜不到' }),
        );
        expect(out.map((i) => i.kind)).toEqual(['gap', 'dropped', 'receiver']);
    });

    it('选中会话：只留这个会话的消息 / 通知 / 请求，调用和元事件没有归属所以不显示', () => {
        const items: ChatItem[] = [
            message({ session: 'group:1' }),
            message({ session: 'group:2' }),
            message({ session: 'private:7' }),
            notice('group:1', 'n1'),
            notice('group:2', 'n2'),
            notice(undefined, '没有归属'),
            request({ requestType: 'group', groupId: 1, userId: 9 }),
            request({ requestType: 'friend', userId: 7 }),
            callItem('get_status'),
            meta(false, 'life'),
            gap(),
        ];
        const g1 = filterItems(items, filter({ session: 'group:1' }));
        expect(g1.map((i) => i.kind)).toEqual(['message', 'notice', 'request', 'gap']);
        const p7 = filterItems(items, filter({ session: 'private:7' }));
        expect(p7.map((i) => i.kind)).toEqual(['message', 'request', 'gap']);
    });

    it('文字搜索不区分大小写，能搜消息正文', () => {
        const items = [message(), message({ segments: [{ type: 'text', data: { text: 'other' } }] })];
        expect(filterItems(items, filter({ text: 'hello' }))).toEqual([items[0]]);
        expect(filterItems(items, filter({ text: '  WORLD ' }))).toEqual([items[0]]);
    });

    it('搜消息预览：图片段搜「图片」、@ 搜名字', () => {
        const withImage = message({ segments: [{ type: 'at', data: { qq: '1', name: 'Alice' } }, { type: 'image', data: {} }] });
        expect(filterItems([withImage], filter({ text: '图片' }))).toHaveLength(1);
        expect(filterItems([withImage], filter({ text: 'alice' }))).toHaveLength(1);
    });

    it('能搜发送者名、QQ 号、消息 id', () => {
        const m = message({ senderName: 'Kyle', senderId: 424242, messageId: 98765 });
        expect(filterItems([m], filter({ text: 'kyle' }))).toHaveLength(1);
        expect(filterItems([m], filter({ text: '4242' }))).toHaveLength(1);
        expect(filterItems([m], filter({ text: '98765' }))).toHaveLength(1);
        expect(filterItems([m], filter({ text: 'nobody' }))).toHaveLength(0);
    });

    it('能搜通知文字、请求验证信息、调用的接口名和参数摘要、元事件文字', () => {
        const items = [notice('group:1', '10 加入了群'), request(), callItem('get_group_member_list', 'group_id=100001'), meta(false, '生命周期：连接建立')];
        expect(filterItems(items, filter({ text: '加入了群' }))).toEqual([items[0]]);
        expect(filterItems(items, filter({ text: '阿强' }))).toEqual([items[1]]);
        expect(filterItems(items, filter({ text: 'MEMBER_LIST' }))).toEqual([items[2]]);
        expect(filterItems(items, filter({ text: '100001' }))).toEqual([items[2]]);
        expect(filterItems(items, filter({ text: '连接建立' }))).toEqual([items[3]]);
    });

    it('搜索与会话、种类叠加生效', () => {
        const items = [
            message({ session: 'group:1', segments: [{ type: 'text', data: { text: 'ping' } }] }),
            message({ session: 'group:2', segments: [{ type: 'text', data: { text: 'ping' } }] }),
            notice('group:1', 'ping notice'),
        ];
        expect(filterItems(items, filter({ session: 'group:1', text: 'ping', kinds: kinds({ notice: false }) }))).toEqual([items[0]]);
    });

    it('心跳在搜索命中时也遵守 showHeartbeat', () => {
        const items = [meta(true, '心跳')];
        expect(filterItems(items, filter({ text: '心跳' }))).toHaveLength(0);
        expect(filterItems(items, filter({ text: '心跳', showHeartbeat: true }))).toHaveLength(1);
    });
});
