// Koishi 沙盒消息的渲染解析：上游消息内容是元素语法串（`<at id="…"/>`、`<img src="…"/>` 这类），
// 试聊页把它拆成段画出来。不认识的元素不丢，降级成灰字占位。

export type KoishiSegment =
    | { kind: 'text'; text: string }
    | { kind: 'at'; id: string; name?: string }
    | { kind: 'img'; src: string }
    | { kind: 'quote'; id?: string }
    | { kind: 'element'; name: string; raw: string };

const TAG =
    /<([a-z][\w-]*)((?:\s+[\w-]+=(?:"[^"]*"|'[^']*'|[^\s>]+))*)\s*(\/?)>|<\/([a-z][\w-]*)>/g;

function attrsOf(raw: string): Record<string, string> {
    const out: Record<string, string> = {};
    const re = /([\w-]+)=("[^"]*"|'[^']*'|[^\s>]+)/g;
    let m: RegExpExecArray | null;
    while ((m = re.exec(raw))) {
        out[m[1]] = m[2].replace(/^["']|["']$/g, '');
    }
    return out;
}

/** 元素串 → 显示段。quote 的内容嵌在标签里（<quote id>…</quote>），这里只记引用本身 */
export function parseKoishiMessage(content: string): KoishiSegment[] {
    const out: KoishiSegment[] = [];
    let at = 0;
    let m: RegExpExecArray | null;
    TAG.lastIndex = 0;
    const pushText = (s: string) => {
        if (s) out.push({ kind: 'text', text: s });
    };
    while ((m = TAG.exec(content))) {
        pushText(content.slice(at, m.index));
        at = m.index + m[0].length;
        const [raw, open, attrText, selfClose, close] = m;
        if (close) continue; // 闭合标签跳过（quote 内容等）
        const attrs = attrsOf(attrText ?? '');
        switch (open) {
            case 'at':
                out.push({ kind: 'at', id: attrs.id ?? '', name: attrs.name });
                break;
            case 'img':
            case 'image':
                if (attrs.src) out.push({ kind: 'img', src: attrs.src });
                else out.push({ kind: 'element', name: open, raw });
                break;
            case 'quote':
                out.push({ kind: 'quote', id: attrs.id });
                break;
            default:
                out.push({ kind: 'element', name: open, raw });
        }
        void selfClose;
    }
    pushText(content.slice(at));
    return out;
}

/** 桌面端试聊用的 platform（沙盒 Bot 绑定到发消息的那条控制台连接；换号 = 换个 platform） */
export const KOISHI_SANDBOX_PLATFORM = 'sandbox:ncd-desktop';

export const KOISHI_SANDBOX_DEFAULT_USER = 'Alice';

/** 频道：私聊 `@用户名`，群聊固定 `#`（上游沙盒的两种模式） */
export function sandboxChannel(mode: 'private' | 'guild', user: string): string {
    return mode === 'guild' ? '#' : `@${user}`;
}
