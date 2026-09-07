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
});
