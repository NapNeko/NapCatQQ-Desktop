import { describe, expect, it } from 'vitest';
import { joinTokens, tokenize } from './syntaxTokens';

describe('syntaxTokens', () => {
    it('json 拼回去等于原文', () => {
        const src = '{\n  "key": "default",\n  "cd": 0\n}\n';
        expect(joinTokens(tokenize(src, 'json'))).toBe(src);
    });

    it('dotenv 中英注释拼回去等于原文', () => {
        const src = '# HTTP鉴权秘钥 仅用于karin自身Api\nHTTP_AUTH_KEY=dg3FzN\n';
        expect(joinTokens(tokenize(src, 'dot_env'))).toBe(src);
    });

    it('提示词：参数单独成块，{{ }} 是字面括号，没配对的照原样', () => {
        const src = '你是 {bot_name}。回 {{"a": 1}} 或 {x}{y}，单独的 } 和 { 不算';
        const toks = tokenize(src, 'prompt');
        expect(joinTokens(toks)).toBe(src);
        expect(toks.filter((t) => t.kind === 'param').map((t) => t.text)).toEqual(['{bot_name}', '{x}', '{y}']);
        expect(toks.filter((t) => t.kind === 'punct').map((t) => t.text)).toEqual(['{{', '}}']);
    });
});
