// 文字段：保留换行、网址可点，超长截断。

import { useMemo } from 'react';
import { useChatView } from '../chatContext';
import { TEXT_RENDER_CAP, URL_RE } from './model';

export function TextSeg({ text }: { text: string }) {
    const { openLink } = useChatView();
    const shown = text.length > TEXT_RENDER_CAP ? text.slice(0, TEXT_RENDER_CAP) : text;
    const parts = useMemo(() => {
        const out: Array<{ link: boolean; text: string }> = [];
        let last = 0;
        URL_RE.lastIndex = 0;
        for (let m = URL_RE.exec(shown); m; m = URL_RE.exec(shown)) {
            if (m.index > last) out.push({ link: false, text: shown.slice(last, m.index) });
            out.push({ link: true, text: m[0] });
            last = m.index + m[0].length;
        }
        if (last < shown.length) out.push({ link: false, text: shown.slice(last) });
        return out;
    }, [shown]);
    return (
        <span className="whitespace-pre-wrap break-words [overflow-wrap:anywhere]">
            {parts.map((p, i) =>
                p.link ? (
                    <a
                        key={i}
                        href={p.text}
                        onClick={(e) => {
                            e.preventDefault();
                            e.stopPropagation();
                            openLink(p.text);
                        }}
                        className="text-info underline-offset-2 hover:underline"
                    >
                        {p.text}
                    </a>
                ) : (
                    <span key={i}>{p.text}</span>
                ),
            )}
            {shown !== text && (
                <span className="text-text-tertiary">
                    …（还有 {text.length - TEXT_RENDER_CAP} 字，在详情里看全文）
                </span>
            )}
        </span>
    );
}
