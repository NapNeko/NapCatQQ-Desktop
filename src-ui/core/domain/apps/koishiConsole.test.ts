import { describe, expect, it } from 'vitest';
import { parseKoishiMessage, sandboxChannel } from './koishiConsole';

describe('parseKoishiMessage', () => {
    it('纯文本原样一段', () => {
        expect(parseKoishiMessage('hello world')).toEqual([{ kind: 'text', text: 'hello world' }]);
    });

    it('元素和文本混排：at / img / 不认识的降级', () => {
        const segs = parseKoishiMessage(
            '你好 <at id="123" name="小明"/> 看图<img src="http://x/a.png"/>完<face id="1"/>',
        );
        expect(segs.map((s) => s.kind)).toEqual(['text', 'at', 'text', 'img', 'text', 'element']);
        expect(segs[1]).toMatchObject({ id: '123', name: '小明' });
        expect(segs[3]).toMatchObject({ src: 'http://x/a.png' });
        expect(segs[2]).toMatchObject({ text: ' 看图' });
    });

    it('quote 记引用、闭合标签不进正文', () => {
        const segs = parseKoishiMessage('<quote id="m1">原话</quote>回应');
        expect(segs[0]).toMatchObject({ kind: 'quote', id: 'm1' });
        expect(segs[segs.length - 1]).toMatchObject({ kind: 'text', text: '回应' });
    });

    it('空串和裸括号不炸', () => {
        expect(parseKoishiMessage('')).toEqual([]);
        expect(parseKoishiMessage('a < b > c')[0]).toMatchObject({
            kind: 'text',
            text: 'a < b > c',
        });
    });

    it('频道：私聊 @用户、群聊 #', () => {
        expect(sandboxChannel('private', 'Alice')).toBe('@Alice');
        expect(sandboxChannel('guild', 'Alice')).toBe('#');
    });
});
