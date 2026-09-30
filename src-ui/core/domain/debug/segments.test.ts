import { describe, expect, it } from 'vitest';
import { messagePreview, normalizeMessage, parseCQ, segmentPreview, type Segment } from './segments';

const seg = (type: string, data: Record<string, unknown> = {}): Segment => ({ type, data });

describe('parseCQ', () => {
    it('纯文字就是一个 text 段', () => {
        expect(parseCQ('hello')).toEqual([seg('text', { text: 'hello' })]);
    });

    it('空串没有消息段', () => {
        expect(parseCQ('')).toEqual([]);
    });

    it('文字夹 CQ 码，参数按逗号切、按第一个等号分键值', () => {
        expect(parseCQ('你好[CQ:at,qq=10001,name=小明]，看图[CQ:image,file=a.jpg,url=http://x/y?a=1&amp;b=2]')).toEqual([
            seg('text', { text: '你好' }),
            seg('at', { qq: '10001', name: '小明' }),
            seg('text', { text: '，看图' }),
            seg('image', { file: 'a.jpg', url: 'http://x/y?a=1&b=2' }),
        ]);
    });

    it('反转义 &amp; &#91; &#93; &#44;（文字和参数值里都要）', () => {
        expect(parseCQ('a&#91;b&#93;c&amp;d')).toEqual([seg('text', { text: 'a[b]c&d' })]);
        expect(parseCQ('[CQ:text,text=1&#44;2&#91;3&#93;&amp;4]')).toEqual([seg('text', { text: '1,2[3]&4' })]);
    });

    it('转义只解一层：&amp;#91; 是字面的 &#91;', () => {
        expect(parseCQ('&amp;#91;')).toEqual([seg('text', { text: '&#91;' })]);
    });

    it('没有参数的 CQ 码、没有等号的参数', () => {
        expect(parseCQ('[CQ:shake]')).toEqual([seg('shake', {})]);
        expect(parseCQ('[CQ:face,id=178,flag]')).toEqual([seg('face', { id: '178', flag: '' })]);
    });

    it('不成对的 [CQ: 当普通文字', () => {
        expect(parseCQ('前 [CQ:at,qq=1 后')).toEqual([seg('text', { text: '前 [CQ:at,qq=1 后' })]);
    });

    it('相邻的 CQ 码之间没有多余的空文字段', () => {
        expect(parseCQ('[CQ:face,id=1][CQ:face,id=2]')).toEqual([seg('face', { id: '1' }), seg('face', { id: '2' })]);
    });

    it('反复调用互不影响（正则的 lastIndex 会复位）', () => {
        expect(parseCQ('[CQ:face,id=1]')).toHaveLength(1);
        expect(parseCQ('[CQ:face,id=1]')).toHaveLength(1);
    });
});

describe('normalizeMessage', () => {
    it('数组格式原样认，缺 data 补空对象', () => {
        expect(normalizeMessage([{ type: 'text', data: { text: 'a' } }, { type: 'shake' }])).toEqual([
            seg('text', { text: 'a' }),
            seg('shake', {}),
        ]);
    });

    it('单个段对象', () => {
        expect(normalizeMessage({ type: 'image', data: { file: 'x' } })).toEqual([seg('image', { file: 'x' })]);
    });

    it('CQ 字符串', () => {
        expect(normalizeMessage('hi[CQ:face,id=1]')).toEqual([seg('text', { text: 'hi' }), seg('face', { id: '1' })]);
    });

    it('数组里混了字符串按文字段处理，混了垃圾就丢', () => {
        expect(normalizeMessage(['x', null, 5, { nope: true }, { type: 'text', data: { text: 'y' } }])).toEqual([
            seg('text', { text: 'x' }),
            seg('text', { text: 'y' }),
        ]);
    });

    it('其它类型给空数组', () => {
        expect(normalizeMessage(null)).toEqual([]);
        expect(normalizeMessage(undefined)).toEqual([]);
        expect(normalizeMessage(42)).toEqual([]);
        expect(normalizeMessage({})).toEqual([]);
    });
});

describe('segmentPreview', () => {
    it('文字原样', () => {
        expect(segmentPreview(seg('text', { text: '你好 [x]' }))).toBe('你好 [x]');
        expect(segmentPreview(seg('text', {}))).toBe('');
    });

    it('@：优先名字，其次 QQ，all 是全体成员', () => {
        expect(segmentPreview(seg('at', { qq: '10001', name: '小明' }))).toBe('@小明');
        expect(segmentPreview(seg('at', { qq: 10001 }))).toBe('@10001');
        expect(segmentPreview(seg('at', { qq: 'all' }))).toBe('@全体成员');
    });

    it('各媒体类型', () => {
        expect(segmentPreview(seg('image'))).toBe('[图片]');
        expect(segmentPreview(seg('face'))).toBe('[表情]');
        expect(segmentPreview(seg('reply'))).toBe('[回复]');
        expect(segmentPreview(seg('record'))).toBe('[语音]');
        expect(segmentPreview(seg('video'))).toBe('[视频]');
        expect(segmentPreview(seg('forward'))).toBe('[聊天记录]');
        expect(segmentPreview(seg('markdown'))).toBe('[Markdown]');
        expect(segmentPreview(seg('poke'))).toBe('[戳一戳]');
        expect(segmentPreview(seg('mface'))).toBe('[表情包]');
    });

    it('文件带文件名，没有名字就只写类型', () => {
        expect(segmentPreview(seg('file', { name: 'report.pdf' }))).toBe('[文件] report.pdf');
        expect(segmentPreview(seg('file', { file: 'fallback.bin' }))).toBe('[文件] fallback.bin');
        expect(segmentPreview(seg('file'))).toBe('[文件]');
    });

    it('卡片带标题：json 翻 prompt / meta，xml 翻 brief', () => {
        expect(segmentPreview(seg('json', { data: JSON.stringify({ prompt: '[分享]好文章', app: 'x' }) }))).toBe('[卡片] [分享]好文章');
        expect(segmentPreview(seg('json', { data: JSON.stringify({ meta: { news: { title: '新闻标题' } } }) }))).toBe('[卡片] 新闻标题');
        expect(segmentPreview(seg('xml', { data: '<msg brief="[图文]标题" serviceID="1"/>' }))).toBe('[卡片] [图文]标题');
        expect(segmentPreview(seg('json', { data: 'not json' }))).toBe('[卡片]');
        expect(segmentPreview(seg('json', {}))).toBe('[卡片]');
    });

    it('不认识的类型写成 [类型]', () => {
        expect(segmentPreview(seg('dice'))).toBe('[dice]');
        expect(segmentPreview(seg('node'))).toBe('[node]');
    });
});

describe('messagePreview', () => {
    it('把各段预览连起来', () => {
        const segs = normalizeMessage('看 [CQ:at,qq=1,name=A] 的[CQ:image,file=a.jpg]和[CQ:face,id=1]');
        expect(messagePreview(segs)).toBe('看 @A 的[图片]和[表情]');
    });

    it('空消息是空串', () => {
        expect(messagePreview([])).toBe('');
    });

    it('同一个数组重复取预览结果一致', () => {
        const segs = [seg('text', { text: 'x' }), seg('image')];
        expect(messagePreview(segs)).toBe('x[图片]');
        expect(messagePreview(segs)).toBe('x[图片]');
    });
});
