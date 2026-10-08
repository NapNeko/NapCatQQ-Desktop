// json / xml 卡片段：标题沿用预览口径，能从 meta 里挖到跳转地址就给一个打开按钮。

import { useMemo } from 'react';
import { ExternalLink, LayoutTemplate } from 'lucide-react';
import { segmentPreview, type Segment } from '../../../core/domain/debug/segments';
import { useChatView } from '../chatContext';
import { isRecord, str } from './model';
import { Card } from './card';

/** json 卡片里常见的跳转地址 */
function cardLink(obj: Record<string, unknown>): string {
    const meta = isRecord(obj.meta) ? obj.meta : {};
    for (const v of Object.values(meta)) {
        if (!isRecord(v)) continue;
        for (const k of ['jumpUrl', 'qqdocurl', 'url']) {
            const s = str(v[k]);
            if (/^https?:\/\//i.test(s)) return s;
        }
    }
    return '';
}

export function RichCard({ seg }: { seg: Segment }) {
    const { openLink } = useChatView();
    const info = useMemo(() => {
        // 标题沿用预览的口径（prompt → meta.*.title / desc → brief），去掉前缀
        const title = segmentPreview(seg).replace(/^\[卡片\]\s*/, '');
        let app = '';
        let link = '';
        if (seg.type === 'json' && typeof seg.data.data === 'string') {
            try {
                const obj: unknown = JSON.parse(seg.data.data);
                if (isRecord(obj)) {
                    app = str(obj.app);
                    link = cardLink(obj);
                }
            } catch {
                // 解析不了就只显示标题
            }
        }
        return { title: title === '[卡片]' ? '' : title, app, link };
    }, [seg]);
    return (
        <Card
            icon={<LayoutTemplate size={16} aria-hidden />}
            title={info.title || (seg.type === 'json' ? 'JSON 卡片' : 'XML 卡片')}
            sub={info.app || (seg.type === 'json' ? 'json' : 'xml')}
        >
            {info.link && (
                <button
                    type="button"
                    onClick={(e) => {
                        e.stopPropagation();
                        openLink(info.link);
                    }}
                    className="mt-0.5 inline-flex w-fit items-center gap-1 text-2xs text-info hover:underline"
                >
                    <ExternalLink size={10} aria-hidden />
                    打开链接
                </button>
            )}
        </Card>
    );
}
