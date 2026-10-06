import { describe, expect, it } from 'vitest';
import {
    buildMessageSegments,
    hasContent,
    mentionLabel,
    mentionQueryAt,
    pruneMentions,
    remainderAfterSend,
} from './composerModel';

describe('composerModel', () => {
    it('按插入过的 @ 把文字切成 at 段；删掉的 @ 不发，手打的 @ 当文字', () => {
        const mentions = [
            { qq: '10003', label: '@阿强' },
            { qq: '10005', label: '@阿强哥' },
            { qq: '99999', label: '@已删掉' },
        ];
        expect(buildMessageSegments('@阿强哥 和 @阿强 看下 @手打的  ', mentions, 42)).toEqual([
            { type: 'reply', data: { id: '42' } },
            { type: 'at', data: { qq: '10005' } },
            { type: 'text', data: { text: ' 和 ' } },
            { type: 'at', data: { qq: '10003' } },
            { type: 'text', data: { text: ' 看下 @手打的' } },
        ]);
    });

    it('认出光标前正在输入的 @，邮箱里的 @ 不算', () => {
        expect(mentionQueryAt('你好 @阿', 5)).toEqual({ start: 3, query: '阿' });
        expect(mentionQueryAt('@', 1)).toEqual({ start: 0, query: '' });
        expect(mentionQueryAt('a@b.com', 7)).toBeNull();
        expect(mentionQueryAt('@阿强 你好', 7)).toBeNull();
    });

    it('发送途中接着打的字，成功后留下', () => {
        expect(remainderAfterSend('你好', '你好')).toBe('');
        expect(remainderAfterSend('你好 再说一句', '你好')).toBe('再说一句');
        expect(remainderAfterSend('改过了', '你好')).toBe('改过了');
    });
    it('重名的 @ 带上 QQ 号；两个同名的人各自对得上', () => {
        expect(mentionLabel('张三', '10006', [], 1)).toBe('@张三');
        expect(mentionLabel('张三', '10006', [], 2)).toBe('@张三(10006)');
        const first = { qq: '10006', label: '@张三' };
        expect(mentionLabel('张三', '10007', [first], 1)).toBe('@张三(10007)');
        expect(mentionLabel('张三', '10006', [first], 1)).toBe('@张三');
        expect(mentionLabel('全体成员', 'all', [], 3)).toBe('@全体成员');
        const second = { qq: '10007', label: '@张三(10007)' };
        expect(buildMessageSegments('@张三 和 @张三(10007)', [first, second])).toEqual([
            { type: 'at', data: { qq: '10006' } },
            { type: 'text', data: { text: ' 和 ' } },
            { type: 'at', data: { qq: '10007' } },
        ]);
    });

    it('光有回复段或只有空白不算有内容；有 @ 就算', () => {
        expect(hasContent(buildMessageSegments('   ', [], 42))).toBe(false);
        expect(hasContent(buildMessageSegments('', []))).toBe(false);
        expect(hasContent(buildMessageSegments('@阿强', [{ qq: '10003', label: '@阿强' }]))).toBe(
            true,
        );
        expect(hasContent(buildMessageSegments('嗨', []))).toBe(true);
    });

    it('文字里删掉了的 @ 不再记着；一个都没删时还是原来那份', () => {
        const mentions = [
            { qq: '10003', label: '@阿强' },
            { qq: '10004', label: '@Bob' },
        ];
        expect(pruneMentions('@阿强 你好', mentions)).toEqual([{ qq: '10003', label: '@阿强' }]);
        expect(pruneMentions('@阿强 @Bob', mentions)).toBe(mentions);
    });
});
