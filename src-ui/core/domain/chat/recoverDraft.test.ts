import { expect, it } from 'vitest';
import { recoverDraft } from './recoverDraft';
import { EMPTY_DRAFT, type Message } from './model';
it('recovers attachments and mentions without losing newer draft text', () => {
    const message: Message = { key: 'failed', session: 'group:1', at: 0, senderId: '99', senderName: '我', mine: true, status: 'failed', segments: [
        { type: 'at', data: { qq: '12' } },
        { type: 'text', data: { text: ' 看看这个' } },
        { type: 'file', data: { name: '说明.txt', file: 'ncd-local-file://D:/说明.txt' } },
    ] };
    const result = recoverDraft(message, { ...EMPTY_DRAFT, text: '新草稿' });
    expect(result.text).toBe('新草稿\n@12 看看这个');
    expect(result.mentions).toEqual([{ qq: '12', label: '@12' }]);
    expect(result.attachments[0].path).toBe('D:/说明.txt');
});
