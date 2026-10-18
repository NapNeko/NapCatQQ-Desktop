import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { preferencesStore } from '../../core/domain/settings/preferencesStore';
import { Dialog, DialogContent, DialogDescription, DialogTitle } from './Dialog';

afterEach(() => {
    cleanup();
    preferencesStore.reset();
});

function Open() {
    return (
        <Dialog open onOpenChange={() => {}}>
            <DialogContent>
                <DialogTitle>收藏请求</DialogTitle>
                <DialogDescription>存下接口和参数</DialogDescription>
                <input aria-label="名字" />
            </DialogContent>
        </Dialog>
    );
}

describe('Dialog', () => {
    it('矮窗口为两层内边距留出空间，缩放窗口后重算滚动高度', async () => {
        preferencesStore.setMotionEnabled(false);
        const height = window.innerHeight;
        const scrollHeight = vi
            .spyOn(HTMLElement.prototype, 'scrollHeight', 'get')
            .mockReturnValue(1000);
        try {
            window.innerHeight = 400;
            render(<Open />);
            const input = await screen.findByRole('textbox', { name: '名字' });
            const inner = input.parentElement!;
            await waitFor(() => expect(inner.style.maxHeight).toBe('304px'));
            window.innerHeight = 280;
            fireEvent(window, new Event('resize'));
            await waitFor(() => expect(inner.style.maxHeight).toBe('184px'));
            window.innerHeight = 1200;
            fireEvent(window, new Event('resize'));
            await waitFor(() => expect(inner.style.maxHeight).toBe(''));
        } finally {
            window.innerHeight = height;
            scrollHeight.mockRestore();
        }
    });

    // 调试台等页面的快捷键靠 [role="dialog"] 在对话框里让路，读屏靠它认出对话框；
    // 内容节点经 asChild 包了一层，Radix 给的属性不能在这一层丢掉
    it('内容节点带着 Radix 的 role、标题和说明', async () => {
        preferencesStore.setMotionEnabled(false);
        render(<Open />);
        const dialog = await screen.findByRole('dialog', { name: '收藏请求' });
        expect(dialog).toHaveAccessibleDescription('存下接口和参数');
        expect(dialog).toContainElement(screen.getByRole('textbox', { name: '名字' }));
        expect(dialog).toHaveAttribute('data-state', 'open');
    });

    it('打开后焦点在对话框里面', async () => {
        preferencesStore.setMotionEnabled(false);
        render(<Open />);
        const dialog = await screen.findByRole('dialog', { name: '收藏请求' });
        await waitFor(() => expect(dialog.contains(document.activeElement)).toBe(true));
    });

    // 没有 DialogDescription 的对话框（比如看大图）本来就不该有 describedby；
    // 留着指向不存在元素的 aria-describedby 会让 Radix 每次打开都 console.warn
    it('没有说明时不带 aria-describedby，也不报 Radix 的说明警告', async () => {
        preferencesStore.setMotionEnabled(false);
        const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
        function WithoutDescription() {
            return (
                <Dialog open onOpenChange={() => {}}>
                    <DialogContent>
                        <DialogTitle>看大图</DialogTitle>
                        <img alt="缩略图" />
                    </DialogContent>
                </Dialog>
            );
        }
        render(<WithoutDescription />);
        const dialog = await screen.findByRole('dialog', { name: '看大图' });
        expect(dialog).not.toHaveAttribute('aria-describedby');
        expect(warn).not.toHaveBeenCalled();
    });
});
