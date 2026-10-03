import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { ChatImageViewer } from './ChatImageViewer';

describe('chat image viewer', () => {
    it('zooms, returns to original size and resets controls for a new image', async () => {
        const close = vi.fn();
        const view = render(<ChatImageViewer src="https://example.test/long.png" onClose={close} />);
        const picture = await screen.findByAltText('消息图片');
        Object.defineProperties(picture, { naturalWidth: { value: 800 }, naturalHeight: { value: 6000 } });
        fireEvent.load(picture);
        fireEvent.click(screen.getByRole('button', { name: '放大图片' }));
        expect(screen.getByRole('button', { name: '原始尺寸' })).toHaveTextContent('125%');
        fireEvent.click(screen.getByRole('button', { name: '原始尺寸' }));
        expect(screen.getByRole('button', { name: '原始尺寸' })).toHaveTextContent('100%');
        fireEvent.keyDown(screen.getByLabelText('图片画布，滚轮缩放，拖动查看'), { key: '+' });
        expect(screen.getByRole('button', { name: '原始尺寸' })).toHaveTextContent('125%');
        view.rerender(<ChatImageViewer src="https://example.test/next.png" onClose={close} />);
        expect(screen.getByRole('button', { name: '放大图片' })).toBeDisabled();
        fireEvent.click(screen.getByRole('button', { name: '关闭图片' }));
        expect(close).toHaveBeenCalledOnce();
    });
    it('shows a usable retry when the full image fails to load', async () => {
        render(<ChatImageViewer src="https://example.test/missing.png" onClose={() => {}} />);
        fireEvent.error(await screen.findByAltText('消息图片'));
        expect(screen.getByText('图片加载失败')).toBeInTheDocument();
        fireEvent.click(screen.getByRole('button', { name: '重试' }));
        expect(screen.getByText('正在加载图片…')).toBeInTheDocument();
    });
});
