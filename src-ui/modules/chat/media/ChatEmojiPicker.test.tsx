import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ChatEmojiPicker } from './ChatEmojiPicker';
import { chatMediaService } from '../../../core/services/chat-media.service';
import type { DebugTarget } from '../../../core/ipc/generated/debug/DebugTarget';
import { qqFaceService } from '../../../core/services/qq-face.service';
import { QQ_FACE_FALLBACK } from '../../../core/domain/chat/qqFaces';
import { QQFace } from './QQFace';

const target: DebugTarget = {
    bot_id: 'bot',
    name: '测试',
    qq_id: 99,
    backend: 'snowluma',
    host: { kind: 'local' },
    running: true,
    online: true,
};
beforeEach(() => {
    vi.spyOn(qqFaceService, 'forAccount').mockResolvedValue({
        faces: QQ_FACE_FALLBACK,
        limited: false,
    });
    vi.spyOn(HTMLElement.prototype, 'offsetHeight', 'get').mockReturnValue(240);
    vi.spyOn(HTMLElement.prototype, 'offsetWidth', 'get').mockReturnValue(340);
});
describe('favorite sticker selection', () => {
    it('finds recently added QQ faces by name and sends the exact numeric id', async () => {
        const select = vi.fn();
        render(<ChatEmojiPicker target={target} onSelect={select} disabledReason="" />);
        expect(screen.getAllByRole('img').length).toBeLessThan(100);
        fireEvent.change(screen.getByRole('textbox', { name: '搜索 QQ 表情' }), {
            target: { value: '被发现了' },
        });
        fireEvent.click(await screen.findByRole('button', { name: '插入QQ 表情 507' }));
        expect(select).toHaveBeenCalledWith(
            expect.objectContaining({ type: 'face', id: '507', name: '被发现了' }),
        );
        fireEvent.change(screen.getByRole('textbox', { name: '搜索 QQ 表情' }), {
            target: { value: '不存在的表情' },
        });
        expect(screen.getByText('没有找到表情')).toBeInTheDocument();
    });
    it('keeps a large collection virtualized and selects the exact URL after scrolling', async () => {
        const urls = Array.from(
            { length: 630 },
            (_, i) => `https://p.qpic.cn/qq_expression/99/sticker-${i}/0`,
        );
        vi.spyOn(chatMediaService, 'favorites').mockResolvedValue(urls);
        const select = vi.fn();
        render(<ChatEmojiPicker target={target} onSelect={select} disabledReason="" />);
        fireEvent.click(screen.getByRole('tab', { name: '收藏表情' }));
        await screen.findByRole('button', { name: '插入收藏表情 1' });
        expect(screen.getAllByRole('img').length).toBeLessThan(40);
        const panel = screen.getByRole('tabpanel');
        panel.scrollTop = 12_084;
        fireEvent.scroll(panel);
        const last = await screen.findByRole('button', { name: '插入收藏表情 630' });
        expect(last.querySelector('img')).toHaveAttribute('src', urls[629]);
        fireEvent.click(last);
        expect(select).toHaveBeenCalledWith(
            expect.objectContaining({ type: 'image', path: urls[629], subType: 1 }),
        );
    });
    it('replaces supported faces on account changes without leaving an unsupported ID selectable', async () => {
        vi.mocked(qqFaceService.forAccount)
            .mockResolvedValueOnce({ faces: [{ id: '486', name: '开学啦2' }], limited: false })
            .mockResolvedValueOnce({ faces: [{ id: '14', name: '微笑' }], limited: false });
        const select = vi.fn();
        const view = render(
            <ChatEmojiPicker target={target} onSelect={select} disabledReason="" />,
        );
        fireEvent.click(await screen.findByRole('button', { name: '插入QQ 表情 486' }));
        expect(select).toHaveBeenCalledWith(expect.objectContaining({ id: '486' }));
        view.rerender(
            <ChatEmojiPicker
                target={{ ...target, qq_id: 100 }}
                onSelect={select}
                disabledReason=""
            />,
        );
        expect(screen.queryByRole('button', { name: '插入QQ 表情 486' })).not.toBeInTheDocument();
        await screen.findByText('1 个表情');
        expect(screen.getByRole('button', { name: '插入QQ 表情 14' })).toBeEnabled();
    });
    it('falls back from the protocol thumbnail to the canonical resource while keeping the same box', () => {
        const view = render(<QQFace id="474" url="https://qq.test/474.png" size={28} />);
        const image = screen.getByRole('img');
        fireEvent.error(image);
        expect(image).toHaveAttribute(
            'src',
            'https://koishi.js.org/QFace/assets/qq_emoji/474/png/474.png',
        );
        expect(image).toHaveStyle({ width: '28px', height: '28px' });
        fireEvent.error(image);
        expect(screen.queryByRole('img')).not.toBeInTheDocument();
        view.rerender(<QQFace id="474" url="https://qq.test/refreshed.png" size={28} />);
        expect(screen.getByRole('img')).toHaveAttribute('src', 'https://qq.test/refreshed.png');
    });
});
