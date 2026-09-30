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

    it('yaml：键、列表、行尾注释分开，拼回去等于原文', () => {
        const src =
            '# 服务端\r\nurl: http://localhost:2536 # 对外地址\nport: 2536\nauth:\n  Authorization: "Bearer a#b"\nmasterQQ:\n  - 10001\n  - - nested\nenable: true\nname:\n';
        const toks = tokenize(src, 'yaml');
        expect(joinTokens(toks)).toBe(src);
        expect(toks.filter((t) => t.kind === 'key').map((t) => t.text)).toEqual([
            'url',
            'port',
            'auth',
            'Authorization',
            'masterQQ',
            'enable',
            'name',
        ]);
        expect(toks.find((t) => t.text === 'http://localhost:2536')?.kind).toBe('plain');
        expect(toks.find((t) => t.text === '"Bearer a#b"')?.kind).toBe('string');
        expect(toks.filter((t) => t.kind === 'comment').map((t) => t.text)).toEqual(['# 服务端', '# 对外地址']);
        expect(toks.find((t) => t.text === '2536')?.kind).toBe('number');
        expect(toks.find((t) => t.text === 'true')?.kind).toBe('bool');
    });

    it('提示词：参数单独成块，{{ }} 是字面括号，没配对的照原样', () => {
        const src = '你是 {bot_name}。回 {{"a": 1}} 或 {x}{y}，单独的 } 和 { 不算';
        const toks = tokenize(src, 'prompt');
        expect(joinTokens(toks)).toBe(src);
        expect(toks.filter((t) => t.kind === 'param').map((t) => t.text)).toEqual(['{bot_name}', '{x}', '{y}']);
        expect(toks.filter((t) => t.kind === 'punct').map((t) => t.text)).toEqual(['{{', '}}']);
    });
});
