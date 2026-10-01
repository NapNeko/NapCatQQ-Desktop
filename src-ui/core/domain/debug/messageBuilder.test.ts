import { describe, expect, it } from 'vitest';
import type { Segment } from './segments';
import {
    assembleMessage,
    blankSegment,
    builderKindLabel,
    hasMessageContent,
    joinDraftText,
    normalizeReplies,
    parseEntryToSegments,
    segmentIssue,
    segmentsToDraft,
    type ComposerEntry,
} from './messageBuilder';

const img = (file: string): Segment => ({ type: 'image', data: { file } });

describe('messageBuilder', () => {
    it('草稿解析：rich 在前，手打文字切成末尾的 text / at', () => {
        const entry: ComposerEntry = {
            text: '@阿强 看下',
            mentions: [{ qq: '10003', label: '@阿强' }],
            rich: [img('http://a/1.png')],
        };
        expect(parseEntryToSegments(entry)).toEqual([
            img('http://a/1.png'),
            { type: 'at', data: { qq: '10003' } },
            { type: 'text', data: { text: ' 看下' } },
        ]);
    });

    it('回复归一：点气泡的回复最前；段里的 reply 提到最前、多余的丢掉', () => {
        expect(normalizeReplies([img('f'), { type: 'reply', data: { id: '7' } }], 42)).toEqual([
            { type: 'reply', data: { id: '42' } },
            img('f'),
        ]);
        expect(
            normalizeReplies([
                { type: 'text', data: { text: 'a' } },
                { type: 'reply', data: { id: '7' } },
                { type: 'reply', data: { id: '8' } },
            ]),
        ).toEqual([
            { type: 'reply', data: { id: '7' } },
            { type: 'text', data: { text: 'a' } },
        ]);
        // 没回复时原样返回（引用也不变）
        const none = [img('f')];
        expect(normalizeReplies(none)).toBe(none);
    });

    it('组装发送内容 = 解析 + 回复归一', () => {
        const entry: ComposerEntry = { text: '好', mentions: [], rich: [img('f')] };
        expect(assembleMessage(entry, 9)).toEqual([{ type: 'reply', data: { id: '9' } }, img('f'), { type: 'text', data: { text: '好' } }]);
    });

    it('能发的判定：光有回复或空白不算，图片单独可发', () => {
        expect(hasMessageContent([{ type: 'reply', data: { id: '1' } }])).toBe(false);
        expect(hasMessageContent([{ type: 'text', data: { text: '   ' } }])).toBe(false);
        expect(hasMessageContent([img('f')])).toBe(true);
        expect(hasMessageContent([{ type: 'at', data: { qq: '10003' } }])).toBe(true);
    });

    it('段换回草稿：只有 text / at 才换得回去', () => {
        expect(segmentsToDraft([img('f')])).toBeNull();
        expect(
            segmentsToDraft([
                { type: 'at', data: { qq: '10003' } },
                { type: 'text', data: { text: '你好' } },
            ]),
        ).toEqual({ text: '@10003 你好', mentions: [{ qq: '10003', label: '@10003' }] });
    });

    it('换草稿时复用输入框里同 QQ 的旧名字，来回编辑不丢「@阿强」', () => {
        const avoid = [{ qq: '10003', label: '@阿强' }];
        expect(segmentsToDraft([{ type: 'at', data: { qq: '10003' } }], avoid)).toEqual({
            text: '@阿强 ',
            mentions: [{ qq: '10003', label: '@阿强' }],
        });
        // 段里自己带 name 时用段里的
        expect(segmentsToDraft([{ type: 'at', data: { qq: '10003', name: '阿强' } }])).toEqual({
            text: '@阿强 ',
            mentions: [{ qq: '10003', label: '@阿强' }],
        });
        // 同名的另一个 QQ 带上 (QQ 号) 区分
        expect(
            segmentsToDraft(
                [
                    { type: 'at', data: { qq: '10006', name: '张三' } },
                    { type: 'at', data: { qq: '10007', name: '张三' } },
                ],
                [],
            ),
        ).toEqual({
            text: '@张三(10006) @张三(10007) ',
            mentions: [
                { qq: '10006', label: '@张三(10006)' },
                { qq: '10007', label: '@张三(10007)' },
            ],
        });
        // 全体
        expect(segmentsToDraft([{ type: 'at', data: { qq: 'all' } }])).toEqual({
            text: '@全体成员 ',
            mentions: [{ qq: 'all', label: '@全体成员' }],
        });
    });

    it('换回的文字排在手打文字前面，衔接处最多补一个空格', () => {
        expect(joinDraftText('', '好')).toBe('好');
        expect(joinDraftText('@阿强 ', '')).toBe('@阿强 ');
        expect(joinDraftText('@阿强 ', '好')).toBe('@阿强 好');
        expect(joinDraftText('@阿强', ' 好')).toBe('@阿强 好');
    });

    it('起步数据与类型名', () => {
        expect(blankSegment('image')).toEqual({ type: 'image', data: { file: '' } });
        expect(blankSegment('poke')).toEqual({ type: 'poke', data: { type: '', id: '' } });
        expect(blankSegment('json')).toEqual({ type: 'json', data: { data: '' } });
        expect(builderKindLabel('markdown')).toBe('Markdown');
        expect(builderKindLabel('weird')).toBe('weird');
    });

    it('段校验：空文字、QQ 号、表情 id、JSON 卡片', () => {
        expect(segmentIssue({ type: 'text', data: { text: '  ' } })).toBe('文字是空的');
        expect(segmentIssue({ type: 'text', data: { text: '好' } })).toBeNull();
        expect(segmentIssue({ type: 'at', data: { qq: '' } })).toBe('还没填 QQ 号');
        expect(segmentIssue({ type: 'at', data: { qq: 'abc' } })).toBe('QQ 号只能是数字，全体填 all');
        expect(segmentIssue({ type: 'at', data: { qq: 'all' } })).toBeNull();
        expect(segmentIssue({ type: 'face', data: { id: 'x' } })).toBe('表情 id 是数字');
        expect(segmentIssue({ type: 'face', data: { id: '14' } })).toBeNull();
        expect(segmentIssue(img(''))).toBe('还没填地址（URL / base64 / 路径）');
        expect(segmentIssue({ type: 'reply', data: { id: '-3' } })).toBeNull();
        expect(segmentIssue({ type: 'poke', data: { type: '126', id: 'x' } })).toBe('类型和 id 都是数字');
        expect(segmentIssue({ type: 'json', data: { data: '{bad' } })).toBe('不是合法的 JSON');
        expect(segmentIssue({ type: 'json', data: { data: '{"a":1}' } })).toBeNull();
        expect(segmentIssue({ type: 'xml', data: { data: '  ' } })).toBe('卡片 XML 是空的');
        // 不认识的段类型不拦
        expect(segmentIssue({ type: 'mface', data: {} })).toBeNull();
    });

    it('解析 → 换回：全是文字时回到出发点', () => {
        const entry: ComposerEntry = {
            text: '@阿强 你好',
            mentions: [{ qq: '10003', label: '@阿强' }],
            rich: [],
        };
        const back = segmentsToDraft(parseEntryToSegments(entry), entry.mentions);
        expect(back).toEqual({ text: entry.text, mentions: entry.mentions });
    });
});
