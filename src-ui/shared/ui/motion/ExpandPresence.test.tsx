import { act, cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import gsap from 'gsap';
import { ExpandPresence } from './ExpandPresence';
import { preferencesStore } from '../../../core/domain/settings/preferencesStore';
import { Dialog, DialogContent, DialogTitle } from '../Dialog';

afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
    preferencesStore.reset();
});

function pauseTimelines() {
    const originalTimeline = gsap.timeline;
    const timelines: gsap.core.Timeline[] = [];
    vi.spyOn(gsap, 'timeline').mockImplementation((vars) => {
        const timeline = originalTimeline({ ...vars, paused: true });
        timelines.push(timeline);
        return timeline;
    });
    vi.spyOn(HTMLElement.prototype, 'scrollHeight', 'get').mockReturnValue(300);
    vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockImplementation(function (
        this: HTMLElement,
    ) {
        const height =
            this.style.height === 'auto' ? 300 : Number.parseFloat(this.style.height) || 0;
        return {
            x: 0,
            y: 0,
            top: 0,
            left: 0,
            bottom: height,
            right: 100,
            width: 100,
            height,
            toJSON() {},
        };
    });
    return timelines;
}

describe('展开动画中断与终态', () => {
    it('收起中途重新展开，从当前高度反转而不先跳回零', () => {
        const timelines = pauseTimelines();
        const view = (visible: boolean) => (
            <ExpandPresence visible={visible}>
                <input />
            </ExpandPresence>
        );
        const { container, rerender } = render(view(true));
        const outer = container.firstElementChild as HTMLDivElement;
        act(() => {
            timelines.at(-1)!.progress(1);
        });
        rerender(view(false));
        act(() => {
            timelines.at(-1)!.progress(0.5);
        });
        const from = Number.parseFloat(outer.style.height);
        expect(from).toBeGreaterThan(0);
        rerender(view(true));
        expect(Number.parseFloat(outer.style.height)).toBeCloseTo(from, 1);
    });

    it('keepMounted 退出完成后标记 closed，父级不会把隐藏内容视作正在收起', () => {
        const timelines = pauseTimelines();
        const view = (visible: boolean) => (
            <ExpandPresence visible={visible} keepMounted>
                <input />
            </ExpandPresence>
        );
        const { container, rerender } = render(view(true));
        const outer = container.firstElementChild as HTMLDivElement;
        act(() => {
            timelines.at(-1)!.progress(1);
        });
        rerender(view(false));
        act(() => {
            timelines.at(-1)!.progress(1);
        });
        expect(outer).toHaveAttribute('hidden');
        expect(outer).toHaveAttribute('data-expand-phase', 'closed');
    });

    it('收起中途关闭动效，立即隐藏保留节点并停止占位', () => {
        const timelines = pauseTimelines();
        const view = (visible: boolean) => (
            <ExpandPresence visible={visible} keepMounted>
                <input aria-label="配置草稿" />
            </ExpandPresence>
        );
        const { container, rerender } = render(view(true));
        const outer = container.firstElementChild as HTMLDivElement;
        act(() => {
            timelines.at(-1)!.progress(1);
        });
        rerender(view(false));
        act(() => {
            timelines.at(-1)!.progress(0.5);
        });
        act(() => {
            preferencesStore.setMotionEnabled(false);
        });
        expect(outer).toHaveAttribute('hidden');
        expect(screen.queryByRole('textbox', { name: '配置草稿' })).not.toBeInTheDocument();
        expect(screen.getByLabelText('配置草稿')).toBeInTheDocument();
    });

    it('弹窗入场中途关闭动效时恢复最终缩放和位移', () => {
        const originalFromTo = gsap.fromTo;
        const tweens: gsap.core.Tween[] = [];
        vi.spyOn(gsap, 'fromTo').mockImplementation((target, from, to) => {
            const tween = originalFromTo(target, from, { ...to, paused: true });
            tweens.push(tween);
            return tween;
        });
        render(
            <Dialog open>
                <DialogContent>
                    <DialogTitle>动态设置</DialogTitle>
                </DialogContent>
            </Dialog>,
        );
        const outer = document.querySelector('[role="dialog"]') as HTMLDivElement;
        const tween = [...tweens].reverse().find((tween) => tween.targets()[0] === outer)!;
        act(() => {
            tween.progress(0.5);
        });
        expect(Number(gsap.getProperty(outer, 'scaleX'))).toBeLessThan(1);
        act(() => {
            preferencesStore.setMotionEnabled(false);
        });
        expect(Number(gsap.getProperty(outer, 'scaleX'))).toBe(1);
        expect(Number(gsap.getProperty(outer, 'x'))).toBe(0);
        expect(Number(gsap.getProperty(outer, 'y'))).toBe(0);
    });
});
