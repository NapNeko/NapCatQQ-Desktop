import { describe, expect, it } from 'vitest';
import type { DebugChannelInfo } from '../../ipc/generated/debug/DebugChannelInfo';
import { exportChannelOf, snippetCode, type SnippetInput } from './codeExport';

function info(patch: Partial<DebugChannelInfo> = {}): DebugChannelInfo {
    return {
        id: { kind: 'http', name: 'main' },
        label: 'HTTP · main :3000',
        can_call: true,
        can_receive: false,
        status: { kind: 'available' },
        endpoint: '127.0.0.1:3000/',
        token_hint: 'ab***kl',
        ...patch,
    };
}

function input(patch: Partial<SnippetInput> = {}): SnippetInput {
    return {
        action: 'send_group_msg',
        params: { group_id: 100001, message: [{ type: 'text', data: { text: '你好' } }] },
        channel: exportChannelOf({ kind: 'http', name: 'main' }, info()),
        ...patch,
    };
}

describe('exportChannelOf', () => {
    it('直连 HTTP 通道：地址就是 endpoint，token 打码值照抄', () => {
        const c = exportChannelOf({ kind: 'http', name: 'main' }, info());
        expect(c).toEqual({ label: 'HTTP · main', baseUrl: 'http://127.0.0.1:3000', placeholderReason: null, tokenHint: 'ab***kl' });
    });

    it('HTTP 通道配了子路径：拼进地址里、尾部斜杠只留一层', () => {
        const c = exportChannelOf({ kind: 'http', name: 'main' }, info({ endpoint: '127.0.0.1:3000/onebot/' }));
        expect(c.baseUrl).toBe('http://127.0.0.1:3000/onebot');
    });

    it('走隧道的 HTTP 通道：用本地转发口，子路径从展示文本里剥出来，并说明端口可能变', () => {
        const c = exportChannelOf(
            { kind: 'http', name: 'main' },
            info({ status: { kind: 'tunneled', local_port: 13001 }, endpoint: '隧道 → 远端 127.0.0.1:3001/onebot' }),
        );
        expect(c.baseUrl).toBe('http://127.0.0.1:13001/onebot');
        expect(c.placeholderReason).toContain('隧道');
    });

    it('内部通道 / WS 通道：占位地址加说明；没有可用通道也是占位', () => {
        const internal = exportChannelOf({ kind: 'internal' }, info({ id: { kind: 'internal' }, endpoint: 'WebUI', token_hint: null }));
        expect(internal.placeholderReason).toContain('内部通道');
        expect(internal.tokenHint).toBeNull();

        const ws = exportChannelOf({ kind: 'ws', name: 'ws1' }, info({ id: { kind: 'ws', name: 'ws1' }, endpoint: '127.0.0.1:3001/' }));
        expect(ws.placeholderReason).toContain('WebSocket');
        expect(ws.baseUrl).toBe('http://127.0.0.1:3000');

        expect(exportChannelOf(null, null).placeholderReason).toContain('没有可用的调用通道');
    });

    it('通道在列表里找不到了（配置改过）：占位地址', () => {
        expect(exportChannelOf({ kind: 'http', name: 'gone' }, null).placeholderReason).not.toBeNull();
    });
});

describe('snippetCode', () => {
    it('curl：POST 地址 /action，带鉴权头，参数是多行 JSON', () => {
        const code = snippetCode('curl', input());
        expect(code).toBe(
            'curl -X POST "http://127.0.0.1:3000/send_group_msg" \\\n' +
                '  -H "Content-Type: application/json" \\\n' +
                '  -H "Authorization: Bearer ab***kl" \\\n' +
                "  -d '{\n" +
                '  "group_id": 100001,\n' +
                '  "message": [\n' +
                '    {\n' +
                '      "type": "text",\n' +
                '      "data": {\n' +
                '        "text": "你好"\n' +
                '      }\n' +
                '    }\n' +
                '  ]\n' +
                "}'",
        );
    });

    it('curl：没有 token 时不带鉴权头，-H 和 -d 之间的续行不能断；参数里的单引号拆成 \'\\\'\'', () => {
        const code = snippetCode('curl', input({ params: { text: "it's" }, channel: { ...input().channel, tokenHint: null } }));
        expect(code).toContain('\\\n  -d');
        expect(code).not.toContain('Authorization');
        expect(code).toContain('"text": "it\'\\\'\'s"');
    });

    it('JavaScript：fetch 形式，参数原样嵌进 JSON.stringify；没有 token 时 headers 只剩 Content-Type', () => {
        const code = snippetCode('javascript', input());
        expect(code).toContain('const response = await fetch("http://127.0.0.1:3000/send_group_msg", {');
        expect(code).toContain('Authorization: "Bearer ab***kl",');
        expect(code).toContain('body: JSON.stringify({');
        expect(code).toContain('"group_id": 100001');
        expect(code).toContain('const reply = await response.json();');

        const bare = snippetCode('javascript', input({ channel: { ...input().channel, tokenHint: null } }));
        expect(bare).not.toContain('Authorization');
        expect(bare).toContain('"Content-Type": "application/json",');
    });

    it('Python：紧凑 JSON 用 json.loads 读回，反斜杠和单引号不会把字面量弄断', () => {
        const code = snippetCode('python', input({ params: { text: 'a\nb', q: "it's" } }));
        expect(code).toContain('import requests');
        expect(code).toContain('"http://127.0.0.1:3000/send_group_msg"');
        // JSON 里的 \n 不能被 Python 当成换行：源码里看到两个反斜杠
        expect(code).toContain("payload = json.loads('{\"text\":\"a\\\\nb\",\"q\":\"it\\'s\"}')");
        expect(code).toContain('headers={"Authorization": "Bearer ab***kl"}');

        // 校验还原回去和原 JSON 一致（模拟 json.loads 的输入）
        const literal = /json\.loads\('(.*)'\)/.exec(code)![1]!;
        const text = literal.replace(/\\\\/g, '\\').replace(/\\'/g, "'");
        expect(JSON.parse(text)).toEqual({ text: 'a\nb', q: "it's" });
    });
});
