import { cleanup, renderHook } from '@testing-library/react';
import gsap from 'gsap';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { preferencesStore } from '../../core/domain/settings/preferencesStore';
import { useMotion } from './useMotion';

const bindings: Array<() => void> = [];

beforeEach(() => {
    preferencesStore.reset();
    gsap.ticker.remove(gsap.updateRoot);
});

afterEach(() => {
    bindings.splice(0).forEach((dispose) => dispose());
    cleanup();
    document.body.replaceChildren();
    preferencesStore.reset();
    gsap.ticker.add(gsap.updateRoot);
});

function advance(seconds: number) {
    const frame = 1 / 60;
    for (let left = seconds; left > 1e-9; left -= frame) {
        gsap.updateRoot(gsap.globalTimeline.time() + Math.min(frame, left));
    }
}

function element() {
    const el = document.createElement('button');
    document.body.appendChild(el);
    return el;
}

function mouse(el: EventTarget, type: string, button = 0) {
    el.dispatchEvent(new MouseEvent(type, { button }));
}

describe('交互动画所有权', () => {
    it('只上浮的悬停不重置其他动画拥有的缩放和透明度', () => {
        const { result } = renderHook(useMotion);
        const el = element();
        gsap.set(el, { scale: 1.3, opacity: 0.4 });
        bindings.push(result.current.bindHover(el, { scale: 1, brightness: false }));

        mouse(el, 'mouseenter');
        advance(0.4);
        mouse(el, 'mouseleave');
        advance(0.4);

        expect(gsap.getProperty(el, 'scaleX')).toBeCloseTo(1.3);
        expect(gsap.getProperty(el, 'opacity')).toBeCloseTo(0.4);
        expect(gsap.getProperty(el, 'y')).toBe(0);
    });

    it('全部反馈关闭时不创建空动画', () => {
        const { result } = renderHook(useMotion);
        const el = element();
        bindings.push(result.current.bindHover(el, { scale: 1, lift: null, brightness: false }));
        mouse(el, 'mouseenter');
        mouse(el, 'mouseleave');
        expect(gsap.getTweensOf(el)).toHaveLength(0);
    });

    it('清理悬停时停止自己的动画并恢复原有滤镜', () => {
        const { result } = renderHook(useMotion);
        const el = element();
        el.style.filter = 'blur(1px)';
        const dispose = result.current.bindHover(el);
        mouse(el, 'mouseenter');
        advance(0.04);
        dispose();
        advance(1);

        expect(gsap.getTweensOf(el)).toHaveLength(0);
        expect(gsap.getProperty(el, 'scaleX')).toBe(1);
        expect(gsap.getProperty(el, 'y')).toBe(0);
        expect(el.style.filter).toBe('blur(1px)');
    });
});

describe('按下反馈的接管和取消', () => {
    it('快速连按只保留当前动画，窗口收到释放后正常归位', () => {
        const { result } = renderHook(useMotion);
        const el = element();
        bindings.push(result.current.bindPress(el));
        for (let i = 0; i < 10; i++) {
            mouse(el, 'mousedown');
            advance(0.015);
            mouse(window, 'mouseup');
            advance(0.015);
            expect(gsap.getTweensOf(el).length).toBeLessThanOrEqual(1);
        }
        advance(1);
        expect(gsap.getProperty(el, 'scaleX')).toBe(1);
        expect(gsap.getTweensOf(el)).toHaveLength(0);
    });

    it.each(['mouseleave', 'blur'])('%s 中断按下后阻尼归位', (type) => {
        const { result } = renderHook(useMotion);
        const el = element();
        bindings.push(result.current.bindPress(el));
        mouse(el, 'mouseenter');
        mouse(el, 'mousedown');
        advance(0.04);
        expect(Number(gsap.getProperty(el, 'scaleX'))).toBeLessThan(1);
        if (type === 'blur') window.dispatchEvent(new Event('blur'));
        else mouse(el, type);
        advance(0.3);
        expect(gsap.getProperty(el, 'scaleX')).toBe(1);
    });

    it('右键不触发压下，清理后迟到的释放不会重新播放', () => {
        const { result } = renderHook(useMotion);
        const el = element();
        const dispose = result.current.bindPress(el);
        mouse(el, 'mousedown', 2);
        expect(gsap.getTweensOf(el)).toHaveLength(0);
        mouse(el, 'mousedown');
        advance(0.04);
        dispose();
        mouse(window, 'mouseup');
        advance(1);
        expect(gsap.getProperty(el, 'scaleX')).toBe(1);
        expect(gsap.getTweensOf(el)).toHaveLength(0);
    });
});
