import { cleanup, render } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { preferencesStore } from '../../../core/domain/settings/preferencesStore';
import { PageTransition } from './PageTransition';
import { DialogStepTransition } from './DialogStepTransition';

type AnimationRecord = {
    frames: Keyframe[];
    options: KeyframeAnimationOptions;
    playState: AnimationPlayState;
    onfinish: (() => void) | null;
    cancel: ReturnType<typeof vi.fn>;
};

const originalAnimate = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'animate');
let animations: AnimationRecord[];

beforeEach(() => {
    preferencesStore.reset();
    animations = [];
    Object.defineProperty(HTMLElement.prototype, 'animate', {
        configurable: true,
        value: (frames: Keyframe[], options: KeyframeAnimationOptions) => {
            const record: AnimationRecord = {
                frames,
                options,
                playState: 'running',
                onfinish: null,
                cancel: vi.fn(() => {
                    record.playState = 'idle';
                }),
            };
            animations.push(record);
            return record as unknown as Animation;
        },
    });
});

afterEach(() => {
    cleanup();
    preferencesStore.reset();
    if (originalAnimate) Object.defineProperty(HTMLElement.prototype, 'animate', originalAnimate);
    else Reflect.deleteProperty(HTMLElement.prototype, 'animate');
});

describe('页面切换响应', () => {
    it('默认档快速交出旧页面，新页面入场不超过 300ms', () => {
        const done = vi.fn();
        const { container, rerender } = render(
            <PageTransition visible onExited={done}>
                旧页
            </PageTransition>,
        );
        expect(Number(animations[0].options.duration)).toBeLessThanOrEqual(300);
        animations[0].playState = 'finished';
        rerender(
            <PageTransition visible={false} onExited={done}>
                旧页
            </PageTransition>,
        );
        const exit = animations[1];
        expect(Number(exit.options.duration)).toBeLessThanOrEqual(120);
        const el = container.firstElementChild as HTMLElement;
        expect(el.style.pointerEvents).toBe('none');
        exit.onfinish?.();
        expect(done).toHaveBeenCalledOnce();
        expect(el.style.visibility).toBe('hidden');
    });

    it('入场未完就切走，从屏幕当前帧开始退场', () => {
        const { container, rerender } = render(<PageTransition visible>内容</PageTransition>);
        const el = container.firstElementChild as HTMLElement;
        el.style.opacity = '0.42';
        el.style.transform = 'translateY(4px)';
        rerender(<PageTransition visible={false}>内容</PageTransition>);
        expect(animations[0].cancel).toHaveBeenCalledOnce();
        expect(animations[1].frames[0]).toEqual({ opacity: '0.42', transform: 'translateY(4px)' });
    });

    it('快速点回原页取消退场，迟到完成不会隐藏页面或回调', () => {
        const done = vi.fn();
        const { container, rerender, unmount } = render(
            <PageTransition visible onExited={done}>
                内容
            </PageTransition>,
        );
        rerender(
            <PageTransition visible={false} onExited={done}>
                内容
            </PageTransition>,
        );
        const exit = animations[1];
        const lateFinish = exit.onfinish;
        rerender(
            <PageTransition visible onExited={done}>
                内容
            </PageTransition>,
        );
        lateFinish?.();
        const el = container.firstElementChild as HTMLElement;
        expect(exit.cancel).toHaveBeenCalledOnce();
        expect(done).not.toHaveBeenCalled();
        expect(el.style.visibility).toBe('');
        expect(el.style.pointerEvents).toBe('');
        unmount();
        expect(animations[2].cancel).toHaveBeenCalledOnce();
    });

    it('关闭动画时直接交出内容', () => {
        preferencesStore.setMotionEnabled(false);
        const done = vi.fn();
        render(
            <PageTransition visible={false} onExited={done}>
                内容
            </PageTransition>,
        );
        expect(animations).toHaveLength(0);
        expect(done).toHaveBeenCalledOnce();
    });
});

describe('弹窗步骤合成动画', () => {
    it('内容重渲不重播，换步骤取消旧动画，卸载释放', () => {
        const { rerender, unmount } = render(
            <DialogStepTransition stepKey="pick">选择</DialogStepTransition>,
        );
        expect(Number(animations[0].options.duration)).toBeLessThanOrEqual(300);
        expect(animations[0].options.fill).toBe('backwards');
        rerender(<DialogStepTransition stepKey="pick">选择结果</DialogStepTransition>);
        expect(animations).toHaveLength(1);
        rerender(<DialogStepTransition stepKey="review">确认</DialogStepTransition>);
        expect(animations[0].cancel).toHaveBeenCalledOnce();
        expect(animations).toHaveLength(2);
        unmount();
        expect(animations[1].cancel).toHaveBeenCalledOnce();
    });
});
