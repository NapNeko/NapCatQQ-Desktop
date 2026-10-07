import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ChatEmojiPicker } from './ChatEmojiPicker';
import { chatMediaService } from '../../../core/services/chat-media.service';
import type { DebugTarget } from '../../../core/ipc/generated/debug/DebugTarget';
import { qqFaceService } from '../../../core/services/qq-face.service';
import { QQ_FACE_FALLBACK } from '../../../core/domain/chat/qqFaces';
import { QQFace } from './QQFace';
import { qqFaceAssetService } from '../../../core/services/qq-face-assets.service';
import { favoriteStickerService } from '../../../core/services/favorite-sticker.service';

const target: DebugTarget = {
    bot_id: 'bot',
    name: '测试',
    qq_id: 99,
    backend: 'snowluma',
    host: { kind: 'local' },
    running: true,
    online: true,
};
function mockPanelScroll(panel: HTMLElement) {
    Object.defineProperty(panel, 'clientHeight', { configurable: true, value: 240 });
    Object.defineProperty(panel, 'scrollHeight', {
        configurable: true,
        get: () =>
            Math.max(
                240,
                Number.parseFloat(
                    (panel.firstElementChild as HTMLElement | null)?.style.height ?? '',
                ) || 240,
            ),
    });
    Object.defineProperty(panel, 'scrollTo', {
        configurable: true,
        value: (options: ScrollToOptions) => {
            panel.scrollTop = options.top ?? 0;
            fireEvent.scroll(panel);
        },
    });
}
beforeEach(() => {
    localStorage.clear();
    vi.spyOn(qqFaceAssetService, 'acquire').mockImplementation(async (url) => ({
        url,
        release: vi.fn(),
    }));
    vi.spyOn(qqFaceAssetService, 'invalidate').mockResolvedValue(undefined);
    vi.spyOn(qqFaceService, 'peekAccount').mockReturnValue(undefined);
    vi.spyOn(favoriteStickerService, 'subscribe').mockReturnValue(() => {});
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
        mockPanelScroll(screen.getByRole('tabpanel'));
        expect(screen.getAllByRole('img').length).toBeLessThan(100);
        expect(screen.queryByRole('textbox', { name: '搜索 QQ 表情' })).not.toBeInTheDocument();
        fireEvent.click(screen.getByRole('button', { name: '搜索表情' }));
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
        vi.spyOn(chatMediaService, 'favoriteDetails').mockResolvedValue(
            urls.map((url) => ({ url, description: '' })),
        );
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
    it('shows metadata categories and preserves recent selections for the current account', async () => {
        vi.mocked(qqFaceService.forAccount).mockResolvedValue({
            faces: [
                { id: '14', name: '微笑', category: '经典' },
                { id: '364', name: '超级赞', super: true, category: '超级' },
            ],
            limited: false,
        });
        render(<ChatEmojiPicker target={target} onSelect={vi.fn()} disabledReason="" />);
        fireEvent.click(await screen.findByRole('button', { name: '超级', exact: true }));
        expect(screen.queryByRole('button', { name: '插入QQ 表情 14' })).not.toBeInTheDocument();
        fireEvent.click(screen.getByRole('button', { name: '插入QQ 表情 364' }));
        fireEvent.click(screen.getByRole('button', { name: '最近', exact: true }));
        expect(screen.getByRole('button', { name: '插入QQ 表情 364' })).toBeEnabled();
        expect(localStorage.getItem('ncd.qq-face-recent.v1')).toContain('364');
    });
    it('falls back from the protocol thumbnail through the CDN mirror while keeping the same box', async () => {
        const view = render(<QQFace id="600000" url="https://qq.test/600000.png" size={28} />);
        const image = screen.getByRole('img');
        await waitFor(() => expect(image).toHaveAttribute('src', 'https://qq.test/600000.png'));
        fireEvent.error(image);
        await waitFor(() =>
            expect(image).toHaveAttribute(
                'src',
                'https://koishi.js.org/QFace/assets/qq_emoji/600000/png/600000.png',
            ),
        );
        expect(image).toHaveStyle({ width: '28px', height: '28px' });
        fireEvent.error(image);
        await waitFor(() =>
            expect(image.getAttribute('src')).toContain('cdn.jsdelivr.net/gh/koishijs/QFace'),
        );
        fireEvent.error(image);
        expect(screen.queryByRole('img')).not.toBeInTheDocument();
        view.rerender(<QQFace id="600000" url="https://qq.test/refreshed.png" size={28} />);
        await waitFor(() =>
            expect(screen.getByRole('img')).toHaveAttribute('src', 'https://qq.test/refreshed.png'),
        );
    });
    it('renders a received super face as an APNG and respects a classic wire override', async () => {
        const view = render(<QQFace id="364" data={{ raw: { faceType: 3 } }} animated />);
        await waitFor(() =>
            expect(screen.getByRole('img')).toHaveAttribute(
                'src',
                'https://koishi.js.org/QFace/assets/qq_emoji/364/apng/364.png',
            ),
        );
        expect(screen.getByRole('img')).toHaveStyle({ width: '72px', height: '72px' });
        view.rerender(<QQFace id="364" data={{ raw: { faceType: 1 } }} animated />);
        await waitFor(() =>
            expect(screen.getByRole('img')).toHaveAttribute('src', '/qq-faces/364.png'),
        );
        expect(screen.getByRole('img')).toHaveStyle({ width: '24px', height: '24px' });
        view.rerender(<QQFace id="364" data={{ id: '364' }} animated />);
        await waitFor(() =>
            expect(screen.getByRole('img')).toHaveAttribute('src', '/qq-faces/364.png'),
        );
        expect(screen.getByRole('img')).toHaveStyle({ width: '24px', height: '24px' });
    });
    it('shows bundled faces immediately without consulting the network cache on a cold start', () => {
        vi.mocked(qqFaceAssetService.acquire).mockReturnValue(new Promise(() => {}));
        const view = render(<QQFace id="277" size={28} />);
        expect(screen.getByRole('img').getAttribute('src')).toContain('qq-faces/277.png');
        expect(qqFaceAssetService.acquire).not.toHaveBeenCalled();
        view.rerender(<QQFace id="364" data={{ large: true }} animated />);
        expect(screen.getByRole('img').getAttribute('src')).toContain('qq-faces/364.png');
        expect(screen.getByRole('img')).toHaveStyle({ width: '72px', height: '72px' });
    });
    it('finds aliases in the original category grid and moves the highlight between matches', async () => {
        vi.mocked(qqFaceService.forAccount).mockResolvedValue({
            faces: [
                { id: '14', name: '微笑', aliases: ['笑脸'], category: '经典' },
                { id: '13', name: '呲牙', aliases: ['笑脸'], category: '经典' },
                { id: '364', name: '超级赞', category: '超级' },
            ],
            limited: false,
        });
        render(<ChatEmojiPicker target={target} onSelect={vi.fn()} disabledReason="" />);
        await screen.findByText('3 个表情');
        mockPanelScroll(screen.getByRole('tabpanel'));
        fireEvent.click(screen.getByRole('button', { name: '搜索表情' }));
        fireEvent.change(screen.getByRole('textbox', { name: '搜索 QQ 表情' }), {
            target: { value: '笑脸' },
        });
        expect(screen.getByRole('button', { name: '插入QQ 表情 364' })).toBeInTheDocument();
        expect(screen.getByRole('button', { name: '插入QQ 表情 14' })).toHaveAttribute(
            'data-search-current',
            'true',
        );
        fireEvent.click(screen.getByRole('button', { name: '下一个匹配表情' }));
        expect(screen.getByRole('button', { name: '插入QQ 表情 13' })).toHaveAttribute(
            'data-search-current',
            'true',
        );
    });
    it('locates favorite descriptions without removing neighboring stickers and reuses the fixed panel', async () => {
        vi.spyOn(chatMediaService, 'favoriteDetails').mockResolvedValue([
            { url: 'https://cdn.example/hello.gif', description: '打招呼' },
            { url: 'https://cdn.example/sleep.gif', description: '晚安' },
        ]);
        render(<ChatEmojiPicker target={target} onSelect={vi.fn()} disabledReason="" />);
        const panel = screen.getByRole('tabpanel');
        mockPanelScroll(panel);
        fireEvent.click(screen.getByRole('tab', { name: '收藏表情' }));
        await screen.findByRole('button', { name: '插入收藏表情 2' });
        fireEvent.click(screen.getByRole('button', { name: '搜索表情' }));
        fireEvent.change(screen.getByRole('textbox', { name: '搜索收藏注释' }), {
            target: { value: '晚安' },
        });
        expect(screen.getByRole('button', { name: '插入收藏表情 1' })).toBeInTheDocument();
        expect(screen.getByRole('button', { name: '插入收藏表情 2' })).toHaveAttribute(
            'data-search-current',
            'true',
        );
        expect(screen.getByRole('tabpanel')).toBe(panel);
    });
});
