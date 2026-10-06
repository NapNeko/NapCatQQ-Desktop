import { describe, expect, it } from 'vitest';
import { checkPrompt, promptDisplayName, promptFields, promptParams } from './maibotPrompts';

describe('promptFields', () => {
    it('follows python formatter', () => {
        expect(promptFields('你好 {bot_name}，{a.b} {c[0]} {d!r} {e:>5} {f:{width}}')).toEqual({
            ok: true,
            fields: ['bot_name', 'a', 'c', 'd', 'e', 'f'],
        });
        expect(promptFields('{{literal}} {x} 例子 {{"k": 1}}')).toEqual({
            ok: true,
            fields: ['x'],
        });
        expect(promptFields('{}')).toEqual({ ok: true, fields: [''] });
        expect(promptFields('单独的 } 不行').ok).toBe(false);
        expect(promptFields('没收尾的 {name').ok).toBe(false);
        expect(promptFields('{a{b}').ok).toBe(false);
    });
});

describe('checkPrompt', () => {
    const def = '{identity}\n{reply_style} 然后 {{JSON}}';

    it('matches the backend messages', () => {
        expect(checkPrompt('{reply_style}{identity}{identity}', def).error).toBeNull();
        expect(checkPrompt('  \n', def).error).toBe('提示词不能是空的');
        expect(checkPrompt('{identity}{reply_style}{}', def).error).toContain('空的 {}');
        const r = checkPrompt('{identity}{bot_name}', def);
        expect(r.error).toBe('少了 {reply_style}；多了 {bot_name}（麦麦给不出这些参数）');
        expect(r.missing).toEqual(['reply_style']);
        expect(r.extra).toEqual(['bot_name']);
    });

    it('lists params once, in order', () => {
        expect(promptParams('{b} {a} {b} {{c}}')).toEqual(['b', 'a']);
    });
});

describe('promptDisplayName', () => {
    it('falls back to the file stem', () => {
        expect(promptDisplayName({ display_name: '回复', name: 'maisaka_replyer.prompt' })).toBe(
            '回复',
        );
        expect(promptDisplayName({ display_name: '', name: 'learn_style.prompt' })).toBe(
            'learn_style',
        );
    });
});
