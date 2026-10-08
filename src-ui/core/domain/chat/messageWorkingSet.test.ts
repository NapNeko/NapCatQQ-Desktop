import { describe, expect, it } from 'vitest';
import type { Message, SessionKey } from './model';
import {
    ARCHIVE_BYTE_LIMIT,
    HISTORY_PAGE_SIZE,
    WORKING_BYTE_LIMIT,
    WORKING_MESSAGE_LIMIT,
    messageBytes,
    retainArchiveMessages,
    retainWorkingMessages,
} from './messageWorkingSet';

let seq = 0;
const msg = (over: Partial<Message> & { key: string; session: SessionKey }): Message => {
    seq += 1;
    return {
        senderId: '20002',
        senderName: '张三',
        at: 1_730_000_000_000 + seq,
        mine: false,
        segments: [{ type: 'text', data: { text: 'hi' } }],
        status: 'sent',
        ...over,
    };
};

/** 同一会话连续 n 条，at 严格递增；下标即时间序 */
const rows = (session: SessionKey, n: number, from = 0): Message[] =>
    Array.from({ length: n }, (_, i) =>
        msg({
            key: `${session.slice(6)}-${from + i}`,
            session,
            at: 1_730_000_000_000 + (from + i) * 1000,
        }),
    );

const keysOf = (list: readonly Message[]) => list.map((m) => m.key);

describe('messageBytes', () => {
    it('同一消息重复调用结果稳定，内容更长字节更大', () => {
        const small = msg({ key: 'b1', session: 'group:10001' });
        const big = msg({
            key: 'b2',
            session: 'group:10001',
            segments: [{ type: 'text', data: { text: 'x'.repeat(5000) } }],
        });
        expect(messageBytes(small)).toBe(messageBytes(small));
        expect(messageBytes(big)).toBeGreaterThan(messageBytes(small));
    });
});

describe('retainWorkingMessages 分页保留', () => {
    it('少量消息全部保留且顺序不变', () => {
        const list = rows('group:10001', 10);
        const kept = retainWorkingMessages(list, 'group:10001');
        expect(keysOf(kept)).toEqual(keysOf(list));
    });

    it('空输入得空结果', () => {
        expect(retainWorkingMessages([])).toEqual([]);
    });

    it('非活跃会话只留最新一页，头部裁掉不算缺口', () => {
        const list = rows('group:10001', HISTORY_PAGE_SIZE + 10);
        const kept = retainWorkingMessages(list, 'group:20002');
        expect(keysOf(kept)).toEqual(keysOf(list.slice(-HISTORY_PAGE_SIZE)));
        expect(kept.every((m) => !m.gapBefore)).toBe(true);
    });

    it('中间既有缺口标记透传，头部裁掉不凭空造缺口', () => {
        const list = rows('group:10001', HISTORY_PAGE_SIZE + 10);
        list[HISTORY_PAGE_SIZE + 1].gapBefore = true;
        const kept = retainWorkingMessages(list, 'group:20002');
        expect(kept.find((m) => m.key === '10001-10')?.gapBefore).toBeFalsy();
        expect(kept.find((m) => m.key === '10001-51')?.gapBefore).toBe(true);
        expect(kept.find((m) => m.key === '10001-52')?.gapBefore).toBeFalsy();
    });

    it('活跃会话阅读锚点保住锚点前后页，中间空洞标 gapBefore', () => {
        const session: SessionKey = 'group:10001';
        const list = rows(session, 300);
        const kept = retainWorkingMessages(list, session, null, {
            session,
            messageKey: `${session.slice(6)}-10`,
            atBottom: false,
        });
        // 锚点在第 0 页：窗口留 0..99，再加活跃会话最后 50 条
        expect(kept).toHaveLength(100 + 50);
        expect(kept.at(-1)?.key).toBe('10001-299');
        expect(kept[100]?.gapBefore).toBe(true);
        expect(kept[101]?.gapBefore).toBeFalsy();
    });

    it('锚点按 messageKey 找不到时退回 messageId 定位', () => {
        const session: SessionKey = 'group:10001';
        const list = rows(session, 300);
        list[130].id = 'msg-130';
        const kept = retainWorkingMessages(list, session, null, {
            session,
            messageKey: '已被改写的旧key',
            messageId: 'msg-130',
            atBottom: false,
        });
        const keptKeys = keysOf(kept);
        expect(keptKeys).toContain('10001-130');
        expect(keptKeys).toContain('10001-100');
        expect(keptKeys).not.toContain('10001-49');
    });

    it('正在加载的会话保住最早一页', () => {
        const session: SessionKey = 'group:10001';
        const list = rows(session, HISTORY_PAGE_SIZE * 2 + 20);
        const idle = retainWorkingMessages(list, null);
        expect(keysOf(idle)).toEqual(keysOf(list.slice(-HISTORY_PAGE_SIZE)));
        const loading = retainWorkingMessages(list, null, session);
        expect(loading).toHaveLength(list.length);
        expect(loading[0]?.key).toBe('10001-0');
    });

    it('会话在 rows 层面乱序传入后仍按时间排好返回', () => {
        const a = rows('group:10001', 5);
        const b = rows('private:20002', 5, 100);
        const shuffled = [...b.slice(2), ...a.slice(2), ...b.slice(0, 2), ...a.slice(0, 2)];
        const kept = retainWorkingMessages(shuffled);
        expect(kept).toHaveLength(10);
        for (let i = 1; i < kept.length; i += 1) expect(kept[i].at).toBeGreaterThan(kept[i - 1].at);
    });

    it('回复目标与未发成功的老消息不受页数裁掉', () => {
        const session: SessionKey = 'group:10001';
        const list = rows(session, 100);
        list[0].id = '9001';
        list[1].status = 'failed';
        list[2].status = 'sending';
        const keptKeys = keysOf(
            retainWorkingMessages(list, null, null, undefined, new Set([`${session}/9001`])),
        );
        expect(keptKeys).toContain('10001-0');
        expect(keptKeys).toContain('10001-1');
        expect(keptKeys).toContain('10001-2');
        expect(keptKeys).not.toContain('10001-3');
    });
});

describe('retainWorkingMessages 预算', () => {
    it('候选超过工作集上限时按新旧淘汰最老会话', () => {
        const sessions = Array.from({ length: 21 }, (_, i) => `group:${20000 + i}` as SessionKey);
        const list = sessions.flatMap((s, i) => rows(s, HISTORY_PAGE_SIZE, i * 1000));
        const kept = retainWorkingMessages(list, null);
        expect(kept).toHaveLength(WORKING_MESSAGE_LIMIT);
        expect(kept.some((m) => m.session === sessions[0])).toBe(false);
        expect(kept.every((m) => !m.gapBefore)).toBe(true);
    });

    it('超预算的本机图片在活跃会话里被单独豁免', () => {
        const session: SessionKey = 'group:10001';
        const huge = (key: string, over: Partial<Message>): Message =>
            msg({
                key,
                session,
                at: 1_730_000_000_000 + 9000,
                segments: [
                    { type: 'image', data: { base64: `base64://${'A'.repeat(4_300_000)}` } },
                ],
                ...over,
            });
        const small = msg({ key: 'keep-me', session, at: 1_730_000_000_000 + 8000 });
        const p0 = huge('oversize-old', { requestId: 'req-0' });
        const p1 = huge('oversize-new', {
            mine: true,
            requestId: 'req-1',
            at: 1_730_000_000_000 + 10_000,
        });
        expect(messageBytes(p0)).toBeGreaterThan(WORKING_BYTE_LIMIT);
        const kept = retainWorkingMessages([small, p0, p1], session);
        const keptKeys = keysOf(kept);
        expect(keptKeys).toContain('oversize-new');
        expect(keptKeys).not.toContain('oversize-old');
        expect(keptKeys).toContain('keep-me');
    });
});

describe('retainArchiveMessages', () => {
    it('按 limit 留连续后缀并保持时间序', () => {
        const list = rows('group:10001', 5);
        expect(keysOf(retainArchiveMessages(list, 2))).toEqual(['10001-3', '10001-4']);
        expect(keysOf(retainArchiveMessages(list))).toEqual(keysOf(list));
        expect(retainArchiveMessages([])).toEqual([]);
    });

    it('归档预算从尾部起算，装不下即停', () => {
        const small = rows('group:10001', 2);
        const hugeText = msg({
            key: 'huge-tail',
            session: 'group:10001',
            segments: [{ type: 'text', data: { text: 'x'.repeat(9_000_000) } }],
        });
        expect(messageBytes(hugeText)).toBeGreaterThan(ARCHIVE_BYTE_LIMIT);
        expect(retainArchiveMessages([...small, hugeText])).toEqual([]);
        expect(keysOf(retainArchiveMessages([hugeText, ...small]))).toEqual(keysOf(small));
    });

    it('超预算的内联图剥离 base64 后入档，原对象不动', () => {
        const original = msg({
            key: 'inline-img',
            session: 'group:10001',
            segments: [
                { type: 'text', data: { text: '看这张' } },
                {
                    type: 'image',
                    data: { file: '/tmp/a.png', base64: `base64://${'A'.repeat(4_300_000)}` },
                },
            ],
        });
        expect(messageBytes(original)).toBeGreaterThan(WORKING_BYTE_LIMIT);
        const [copy] = retainArchiveMessages([original]);
        const image = copy?.segments[1];
        expect(image?.data.base64).toBeUndefined();
        expect(image?.data.file).toBe('/tmp/a.png');
        expect(copy?.segments[0].data.text).toBe('看这张');
        expect(typeof original.segments[1].data.base64).toBe('string');
    });

    it('小媒体原样入档，不产生副本改动', () => {
        const list = rows('group:10001', 3);
        list[1].segments.push({ type: 'image', data: { file: '/data/face.png' } });
        const kept = retainArchiveMessages(list);
        expect(keysOf(kept)).toEqual(keysOf(list));
        expect(kept[1].segments[1].data.file).toBe('/data/face.png');
    });
});
