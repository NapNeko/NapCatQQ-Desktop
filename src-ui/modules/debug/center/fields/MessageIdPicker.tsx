// message_id：从右栏最近收到的 20 条消息里挑（预览 + 发送人），也可以直接填。

import { useMemo } from 'react';
import { useDebugChat } from '../../../../hooks/debug/debugEventStore';
import { messagePreview } from '../../../../core/domain/debug/segments';
import type { DebugTarget } from '../../../../core/ipc/generated/debug/DebugTarget';
import { PickerCombo, type PickerOption } from './PickerCombo';
import type { FieldProps } from './fieldKit';

const RECENT = 20;

function clock(ms: number): string {
    const d = new Date(ms);
    const pad = (n: number) => String(n).padStart(2, '0');
    return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}

export function MessageIdPicker({ target, ...field }: FieldProps & { target: DebugTarget | null }) {
    const chat = useDebugChat(target?.bot_id ?? null);
    const options = useMemo<PickerOption[]>(() => {
        const out: PickerOption[] = [];
        // 从新往旧找，找够 20 条就停：时间线可能有几千条
        for (let i = chat.items.length - 1; i >= 0 && out.length < RECENT; i -= 1) {
            const item = chat.items[i]!;
            if (item.kind !== 'message' || item.messageId === undefined) continue;
            const session = chat.sessions[item.session]?.name;
            const who = item.direction === 'out' ? '我' : item.senderName || String(item.senderId);
            out.push({
                id: item.messageId,
                label: messagePreview(item.segments) || '（空消息）',
                hint: [who, session, clock(item.at)].filter(Boolean).join(' · '),
            });
        }
        return out;
    }, [chat.items, chat.sessions]);

    return (
        <PickerCombo
            {...field}
            noun="最近消息"
            options={options}
            unavailable={target ? null : '先在顶栏选一个 Bot；也可以直接填 message_id'}
            emptyText="右栏还没收到带 message_id 的消息，直接填也行"
            placeholder="填 message_id，或从最近的消息里挑"
        />
    );
}
