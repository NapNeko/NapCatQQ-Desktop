import { act, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ChatImageViewer } from './ChatImageViewer';

let observers: Map<Element, (width: number, height: number) => void>;
const originalResizeObserver = globalThis.ResizeObserver;
beforeEach(() => {
    observers = new Map();
    globalThis.ResizeObserver = class {
        constructor(private callback: ResizeObserverCallback) {}
        observe(target: Element) {
            const resize = (width: number, height: number) =>
                this.callback(
                    [{ target, contentRect: { width, height } } as ResizeObserverEntry],
                    this as unknown as ResizeObserver,
                );
            observers.set(target, resize);
            resize(1000, 650);
        }
        unobserve() {}
        disconnect() {}
    };
});
afterEach(() => {
    globalThis.ResizeObserver = originalResizeObserver;
});

describe('chat image viewer', () => {
    it('zooms, returns to original size and resets controls for a new image', async () => {
        const close = vi.fn();
        const view = render(
            <ChatImageViewer src="https://example.test/long.png" onClose={close} />,
        );
        const picture = await screen.findByAltText('消息图片');
        Object.defineProperties(picture, {
            naturalWidth: { value: 800 },
            naturalHeight: { value: 6000 },
        });
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
    it('fits to fractional canvas dimensions instead of rounded client sizes', async () => {
        render(<ChatImageViewer src="https://example.test/landscape.png" onClose={() => {}} />);
        const picture = await screen.findByAltText('消息图片');
        const canvas = screen.getByLabelText('图片画布，滚轮缩放，拖动查看');
        Object.defineProperties(canvas, {
            clientWidth: { value: 1000 },
            clientHeight: { value: 651 },
        });
        Object.defineProperties(picture, {
            naturalWidth: { value: 1600 },
            naturalHeight: { value: 1200 },
        });
        act(() => observers.get(canvas)?.(1000.25, 650.75));
        fireEvent.load(picture);
        expect(parseFloat(picture.style.height)).toBeCloseTo(602.75);

        act(() => observers.get(canvas)?.(1200.25, 750.75));
        expect(parseFloat(picture.style.height)).toBeCloseTo(702.75);
        fireEvent.click(screen.getByRole('button', { name: '原始尺寸' }));
        act(() => observers.get(canvas)?.(1000.25, 650.75));
        expect(picture).toHaveStyle({ width: '1600px', height: '1200px' });
    });
    it('waits for a usable canvas before displaying a decoded image', async () => {
        render(<ChatImageViewer src="https://example.test/small.png" onClose={() => {}} />);
        const picture = await screen.findByAltText('消息图片');
        const canvas = screen.getByLabelText('图片画布，滚轮缩放，拖动查看');
        act(() => observers.get(canvas)?.(0, 0));
        Object.defineProperties(picture, {
            naturalWidth: { value: 333 },
            naturalHeight: { value: 136 },
        });
        fireEvent.load(picture);
        expect(screen.getByRole('button', { name: '放大图片' })).toBeDisabled();
        act(() => observers.get(canvas)?.(1000.25, 650.75));
        expect(screen.getByRole('button', { name: '放大图片' })).toBeEnabled();
        expect(picture).toHaveStyle({ width: '333px', height: '136px' });
    });
});
