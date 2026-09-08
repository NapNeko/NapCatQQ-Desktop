import { errorBarContent } from '../../core/domain/ui/errorBarCopy';
import { pushInfoBar } from './globalInfoBarStore';

export function pushErrorBar(args: {
    key?: string;
    title: string;
    raw?: string | null;
    content?: string;
    autoDismissMs?: number;
}): void {
    if (args.raw) console.error(`[ui] ${args.title}:`, args.raw);
    pushInfoBar({
        key: args.key,
        tone: 'danger',
        title: args.title,
        content: args.content ?? errorBarContent(args.raw),
        autoDismissMs: args.autoDismissMs,
    });
}
