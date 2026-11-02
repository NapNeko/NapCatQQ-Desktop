import {
    inlineImageService,
    isInlineImageReference,
} from '../../../core/services/inline-image.service';
import { id, record, text, type Account, type Message } from '../../../core/domain/chat/model';
import { errorText } from '../../../core/domain/errors';
import { normalizeMessage, type Segment } from '../../../core/domain/debug/segments';
import type { DebugTarget } from '../../../core/ipc/generated/debug/DebugTarget';

/** 管理内联图片引用及其在传输边界的转换。 */
export class ChatMediaCodec {
    constructor(
        private getTarget: () => DebugTarget,
        private reportError: (error: string) => void,
    ) {}
    private get target() {
        return this.getTarget();
    }
    syncAccount(account: Account): Account {
        const sources = new Map<string, boolean>();
        const mark = (message: Message): Message => {
            let changed = false;
            const segments = message.segments.map((segment) => {
                const reference = [
                    segment.data.inline_ref,
                    segment.data.local_file,
                    segment.data.file,
                ].find(isInlineImageReference);
                if (!reference) return segment;
                sources.set(reference, !!sources.get(reference) || message.status !== 'sent');
                if (message.id && segment.data.source_message_id !== message.id) {
                    changed = true;
                    return { ...segment, data: { ...segment.data, source_message_id: message.id } };
                }
                return segment;
            });
            return changed ? { ...message, segments } : message;
        };
        const sync = (rows: Message[]): Message[] => {
            let changed: Message[] | undefined;
            rows.forEach((message, index) => {
                const next = mark(message);
                if (next === message) return;
                changed ??= rows.slice();
                changed[index] = next;
            });
            return changed ?? rows;
        };
        const messages = sync(account.messages);
        const archiveMessages =
            account.archiveMessages === account.messages
                ? messages
                : account.archiveMessages && sync(account.archiveMessages);
        if (messages !== account.messages || archiveMessages !== account.archiveMessages)
            account = { ...account, messages, archiveMessages };
        for (const [reference, protectedSource] of sources)
            inlineImageService.protect(
                this.target.bot_id,
                String(this.target.qq_id),
                reference,
                protectedSource,
            );
        return account;
    }
    externalizeSegments(segments: Segment[], created: string[] = []): Segment[] {
        return segments.map((segment) => {
            if (segment.type !== 'image' && segment.type !== 'mface') return segment;
            const data = { ...segment.data };
            const converted = new Map<string, string>();
            let changed = false;
            for (const field of ['file', 'local_file', 'base64', 'url']) {
                const source = text(data[field]);
                if (
                    source.length < 256 * 1024 ||
                    !(
                        field === 'base64' ||
                        /^(base64:\/\/|data:image\/[^;,]+;base64,)/i.test(source)
                    )
                )
                    continue;
                let reference = converted.get(source);
                if (!reference) {
                    reference = inlineImageService.capture(
                        this.target.bot_id,
                        String(this.target.qq_id),
                        source,
                        (reference) => created.push(reference),
                    );
                    converted.set(source, reference);
                }
                data[field] = reference;
                data.inline_ref = reference;
                changed = true;
            }
            return changed ? { ...segment, data } : segment;
        });
    }
    externalizeRows(rows: readonly unknown[]): unknown[] {
        return rows.map((raw) => {
            const row = record(raw);
            if (!row.message && !row.raw_message) return raw;
            const created: string[] = [];
            try {
                const segments = this.externalizeSegments(
                    normalizeMessage(row.message ?? row.raw_message),
                    created,
                );
                return { ...row, message: segments };
            } catch (error) {
                for (const reference of created)
                    inlineImageService.discard(
                        this.target.bot_id,
                        String(this.target.qq_id),
                        reference,
                    );
                this.reportError(errorText(error));
                const segments = normalizeMessage(row.message ?? row.raw_message).map((segment) => {
                    if (segment.type !== 'image' && segment.type !== 'mface') return segment;
                    const data: Record<string, unknown> = {
                        ...segment.data,
                        source_message_id: id(row.message_id),
                    };
                    for (const field of ['file', 'local_file', 'url', 'base64'])
                        if (
                            /^(base64:\/\/|data:image\/)/i.test(text(data[field])) ||
                            field === 'base64'
                        )
                            delete data[field];
                    return { ...segment, data };
                });
                return { ...row, message: segments };
            }
        });
    }
    materializeSegments(segments: Segment[]): Segment[] {
        return segments.map((segment) => {
            if (segment.type !== 'image' && segment.type !== 'mface') return segment;
            const data = { ...segment.data };
            for (const field of ['file', 'local_file', 'base64', 'url']) {
                const value = data[field];
                if (!isInlineImageReference(value)) continue;
                const source = inlineImageService.source(
                    this.target.bot_id,
                    String(this.target.qq_id),
                    value,
                );
                if (!source) throw new Error('原图片缓存已失效，请重新添加后发送');
                data[field] = field === 'base64' ? source.slice('base64://'.length) : source;
            }
            delete data.inline_ref;
            delete data.source_message_id;
            return { ...segment, data };
        });
    }
}
