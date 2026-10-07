import { expect, it } from 'vitest';
import { recoverDraft } from './recoverDraft';
import { EMPTY_DRAFT, type Message } from './model';
it('recovers attachments and mentions without losing newer draft text', () => {
    const message: Message = {
        key: 'failed',
        session: 'group:1',
        at: 0,
        senderId: '99',
        senderName: '我',
        mine: true,
        status: 'failed',
        segments: [
            { type: 'at', data: { qq: '12' } },
            { type: 'text', data: { text: ' 看看这个' } },
            { type: 'file', data: { name: '说明.txt', file: 'ncd-local-file://D:/说明.txt' } },
            { type: 'image', data: { file: 'ncd-inline-image://source' } },
        ],
    };
    const result = recoverDraft(message, { ...EMPTY_DRAFT, text: '新草稿' });
    expect(result.text).toBe('新草稿\n@12 看看这个');
    expect(result.mentions).toEqual([{ qq: '12', label: '@12' }]);
    expect(result.attachments[0]).toMatchObject({ path: 'D:/说明.txt' });
    expect(result.attachments[1]).toMatchObject({
        path: 'ncd-inline-image://source',
        name: '粘贴的图片.png',
    });
});

it('recovers QQ faces and favorite image URLs while rejecting unsafe protocols', () => {
    const message: Message = {
        key: 'failed',
        session: 'group:1',
        at: 0,
        senderId: '99',
        senderName: '我',
        mine: true,
        status: 'failed',
        segments: [
            { type: 'face', data: { id: 14 } },
            { type: 'face', data: { id: '../invalid' } },
            { type: 'image', data: { file: 'https://cdn.example/favorite.gif', sub_type: 1 } },
            { type: 'image', data: { file: 'javascript:alert(1)' } },
            { type: 'image', data: { file: 'file:///etc/passwd' } },
        ],
    };
    expect(recoverDraft(message, EMPTY_DRAFT).attachments).toMatchObject([
        { type: 'face', id: '14' },
        { type: 'image', path: 'https://cdn.example/favorite.gif', subType: 1 },
    ]);
});
