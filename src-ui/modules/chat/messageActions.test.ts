import { describe, expect, it } from 'vitest';
import type { Draft, Message } from '../../core/domain/chat/model';
import { buildMessageSegments } from '../../core/domain/debug/composerModel';
import { mentionMessageSender } from './messageActions';

const message: Message = { key: 'group:1/2', session: 'group:1', id: '2', senderId: '20', senderName: '小明', at: 0, mine: false, segments: [], status: 'sent' };
const draft: Draft = { text: '未完成的消息', mentions: [], attachments: [{ key: 'a', type: 'file', name: '备注.txt', path: 'C:/备注.txt' }], reply: { id: '9', name: '小李', preview: '上一条' } };

describe('mentionMessageSender', () => {
    it('keeps the text, attachments and reply while appending a real mention', () => {
        const next = mentionMessageSender(draft, message);
        expect(next.text).toBe('未完成的消息 @小明 ');
        expect(next.reply).toBe(draft.reply);
        expect(next.attachments).toBe(draft.attachments);
        expect(buildMessageSegments(next.text, next.mentions ?? [])).toEqual([
            { type: 'text', data: { text: '未完成的消息 ' } }, { type: 'at', data: { qq: '20' } },
        ]);
        expect(draft.text).toBe('未完成的消息');
    });
    it('disambiguates another member with the same display name', () => {
        const next = mentionMessageSender({ ...draft, text: '@小明 ', mentions: [{ qq: '10', label: '@小明' }] }, message);
        expect(next.text).toBe('@小明 @小明(20) ');
        expect(buildMessageSegments(next.text, next.mentions ?? []).filter(segment => segment.type === 'at')).toEqual([
            { type: 'at', data: { qq: '10' } }, { type: 'at', data: { qq: '20' } },
        ]);
    });
    it('does not turn existing manually typed names into mentions', () => {
        const next = mentionMessageSender({ ...draft, text: '字面量 @小明 和 @小明(20)' }, message);
        expect(next.text).toBe('字面量 @小明 和 @小明(20) @小明(20)[2] ');
        expect(buildMessageSegments(next.text, next.mentions ?? [])).toEqual([
            { type: 'text', data: { text: '字面量 @小明 和 @小明(20) ' } }, { type: 'at', data: { qq: '20' } },
        ]);
    });
    it('prunes deleted mentions and does not duplicate mention metadata on repeated actions', () => {
        const first = mentionMessageSender({ ...draft, mentions: [{ qq: '30', label: '@已删除' }] }, message);
        const next = mentionMessageSender(first, message);
        expect(next.text).toBe('未完成的消息 @小明 @小明 ');
        expect(next.mentions).toEqual([{ qq: '20', label: '@小明' }]);
    });
    it('qualifies known duplicate names and preserves a trailing newline', () => {
        const next = mentionMessageSender({ ...draft, text: '下一行\n' }, message, 2);
        expect(next.text).toBe('下一行\n@小明(20) ');
    });
});
