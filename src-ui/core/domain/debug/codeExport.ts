// 「导出调用代码」：按当前请求（动作 + 参数）和实际生效的调用通道生成 curl / JavaScript / Python 片段。
//
// 片段一律按 HTTP 调用形式生成（OneBot 各实现都认 POST /<action>）；内部通道 / WS 通道没有对应的
// HTTP 地址，用占位地址加说明。token 不明文出后端：能写的只有打码值（token_hint），用之前自己换掉。

import type { DebugChannelId } from '../../ipc/generated/debug/DebugChannelId';
import type { DebugChannelInfo } from '../../ipc/generated/debug/DebugChannelInfo';

export type SnippetLang = 'curl' | 'javascript' | 'python';

export const SNIPPET_LANGS: ReadonlyArray<{ id: SnippetLang; label: string }> = [
    { id: 'curl', label: 'curl' },
    { id: 'javascript', label: 'JavaScript' },
    { id: 'python', label: 'Python' },
];

/** 没有可用地址时的占位：最常见也最不影响辨认的本地默认口 */
const PLACEHOLDER_BASE = 'http://127.0.0.1:3000';

export interface ExportChannel {
    /** 通道短名，对话框里写「按 X 生成」 */
    label: string;
    /** 片段里用的地址；拿不到真实地址时是占位 */
    baseUrl: string;
    /** 地址为什么是占位 / 为什么按 HTTP 等价形式生成；真实 HTTP 地址时是 null */
    placeholderReason: string | null;
    /** token 的打码值；这条通道没配 token 是 null */
    tokenHint: string | null;
}

const placeholder = (label: string, reason: string, tokenHint: string | null = null): ExportChannel => ({
    label,
    baseUrl: PLACEHOLDER_BASE,
    placeholderReason: reason,
    tokenHint,
});

/** 「隧道 → 远端 host:port/path」里的子路径；认不出（格式变了、没配）按 / */
function tunneledPath(endpoint: string | null): string {
    const m = /[^:]+:\d+(\/\S*)?$/.exec(endpoint ?? '');
    return m?.[1] ?? '';
}

/** 「127.0.0.1:3000/onebot」→「http://127.0.0.1:3000/onebot」（尾部斜杠去掉，拼动作时统一加） */
function httpBaseOf(endpoint: string): string {
    const raw = endpoint.trim();
    const withScheme = /^https?:\/\//i.test(raw) ? raw : `http://${raw}`;
    return withScheme.replace(/\/+$/, '');
}

/**
 * 片段按哪条通道生成。`resolved` 是「自动」落定后真正要用的通道（null = 现在没有可用的）；
 * `info` 是通道列表里那一项（找不到说明配置已经改了）。
 */
export function exportChannelOf(resolved: DebugChannelId | null, info: DebugChannelInfo | null): ExportChannel {
    if (resolved === null) {
        return placeholder('（没有可用通道）', '现在没有可用的调用通道；地址是占位，换成你的 HTTP 服务地址后再用');
    }
    const hint = info?.token_hint ?? null;
    switch (resolved.kind) {
        case 'auto':
            // 落定前被拦在 null 分支了；真走到这里说明数据前后不一致，同样按占位处理
            return placeholder('自动', '现在没有可用的调用通道；地址是占位，换成你的 HTTP 服务地址后再用');
        case 'internal':
            return placeholder(
                '内部通道',
                '内部通道走 Bot WebUI 自带的调试接口，没有独立的 HTTP 地址；片段按 HTTP 形式生成，地址是占位，换成你的 HTTP 服务地址',
                hint,
            );
        case 'ws':
            return placeholder(
                `WS · ${resolved.name}`,
                '这条通道是 WebSocket；片段按 HTTP 等价形式生成，地址是占位，换成实际开着的 HTTP 服务地址',
                hint,
            );
        case 'http': {
            const label = `HTTP · ${resolved.name}`;
            // 走隧道的：本地转发口就是此刻真正调通的地址，子路径从展示文本里剥出来
            if (info?.status.kind === 'tunneled') {
                return {
                    label,
                    baseUrl: `http://127.0.0.1:${info.status.local_port}${tunneledPath(info.endpoint)}`,
                    placeholderReason: '通道走 SSH 隧道：地址是此刻的本地转发口，隧道重开后端口可能变',
                    tokenHint: hint,
                };
            }
            // 直连的：endpoint 就是 host:port/path
            if (info?.endpoint && !info.endpoint.includes('→')) {
                return { label, baseUrl: httpBaseOf(info.endpoint), placeholderReason: null, tokenHint: hint };
            }
            return placeholder(label, '这条 HTTP 通道现在拿不到地址；地址是占位，换成你的 HTTP 服务地址', hint);
        }
    }
}

export interface SnippetInput {
    action: string;
    params: Record<string, unknown>;
    channel: ExportChannel;
}

function pretty(v: Record<string, unknown>): string {
    return JSON.stringify(v, null, 2);
}

/** 除第一行外每行补缩进：嵌进函数调用里第一层不再多缩 */
function indent(text: string, spaces: number): string {
    const pad = ' '.repeat(spaces);
    return text
        .split('\n')
        .map((line, i) => (i === 0 ? line : pad + line))
        .join('\n');
}

function curlSnippet(url: string, bearer: string | null, params: Record<string, unknown>): string {
    // curl 的 -d 用单引号包：内容里的单引号拆成 '\'' 不会断
    const body = pretty(params).replace(/'/g, "'\\''");
    const parts = [
        `curl -X POST "${url}"`,
        `-H "Content-Type: application/json"`,
        ...(bearer ? [`-H "Authorization: Bearer ${bearer}"`] : []),
        `-d '${body}'`,
    ];
    return parts.join(' \\\n  ');
}

function jsSnippet(url: string, bearer: string | null, params: Record<string, unknown>): string {
    const headers = ['"Content-Type": "application/json",'];
    if (bearer) headers.push(`Authorization: "Bearer ${bearer}",`);
    const lines = [
        `const response = await fetch("${url}", {`,
        `  method: "POST",`,
        `  headers: {`,
        ...headers.map((h) => `    ${h}`),
        `  },`,
        `  body: JSON.stringify(${indent(pretty(params), 4)}),`,
        `});`,
        `const reply = await response.json();`,
        `console.log(reply);`,
    ];
    return lines.join('\n');
}

function pythonSnippet(url: string, bearer: string | null, params: Record<string, unknown>): string {
    // 紧凑 JSON 塞进单引号字面量：先保反斜杠（JSON 里的 \n 不能被 Python 吃掉），再转义单引号
    const compact = JSON.stringify(params).replace(/\\/g, '\\\\').replace(/'/g, "\\'");
    const auth = bearer ? `,\n    headers={"Authorization": "Bearer ${bearer}"}` : '';
    return [
        'import json',
        '',
        'import requests',
        '',
        `payload = json.loads('${compact}')`,
        `reply = requests.post(`,
        `    "${url}",`,
        `    json=payload${auth},`,
        `).json()`,
        `print(reply)`,
    ].join('\n');
}

/** 生成一个语言的片段 */
export function snippetCode(lang: SnippetLang, input: SnippetInput): string {
    const url = `${input.channel.baseUrl}/${input.action}`;
    const bearer = input.channel.tokenHint;
    switch (lang) {
        case 'curl':
            return curlSnippet(url, bearer, input.params);
        case 'javascript':
            return jsSnippet(url, bearer, input.params);
        case 'python':
            return pythonSnippet(url, bearer, input.params);
    }
}
