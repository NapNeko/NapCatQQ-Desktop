// @vitest-environment jsdom

import { act, cleanup, render } from '@testing-library/react';
import gsap from 'gsap';
import { MessageSquare, type LucideProps } from 'lucide-react';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { preferencesStore } from '../../../core/domain/settings/preferencesStore';
import { MotionIcon } from './MotionIcon';

beforeEach(() => {
    gsap.ticker.remove(gsap.updateRoot);
    preferencesStore.reset();
});

afterEach(() => {
    cleanup();
    preferencesStore.reset();
    gsap.ticker.add(gsap.updateRoot);
});

function advance(seconds: number) {
    act(() => {
        const frame = 1 / 60;
        for (let left = seconds; left > 1e-9; left -= frame) {
            gsap.updateRoot(gsap.globalTimeline.time() + Math.min(frame, left));
        }
    });
}

function renderEnteringIcon() {
    const view = render(<MotionIcon icon={MessageSquare} motion="pulse" enterKey="chat" />);
    const wrap = view.container.firstElementChild as HTMLSpanElement;
    const path = wrap.querySelector<SVGPathElement>('path')!;
    return { ...view, wrap, path };
}

function expectRestored(wrap: HTMLSpanElement, path: SVGPathElement) {
    expect(wrap.style.opacity).toBe('');
    expect(wrap.style.transform).toBe('');
    expect(path.style.strokeDasharray).toBe('');
    expect(path.style.strokeDashoffset).toBe('');
}

function pointer(wrap: HTMLSpanElement, event: 'mouseenter' | 'mouseleave') {
    act(() => {
        wrap.dispatchEvent(new MouseEvent(event));
    });
}

describe('MotionIcon 入场与中断', () => {
    it('Lucide 继承的描边参与入场，完成后清理内联样式并恢复 CSS 循环', () => {
        const { wrap, path } = renderEnteringIcon();
        expect(path.getAttribute('stroke')).toBeNull();
        expect(wrap.querySelector('svg')?.getAttribute('stroke')).toBe('currentColor');
        expect(Number(path.style.strokeDasharray)).toBeGreaterThan(0);
        expect(wrap.classList.contains('transition-[opacity]')).toBe(false);

        advance(1);

        expectRestored(wrap, path);
        expect(wrap.classList.contains('ndf-icon-loop--pulse')).toBe(true);
    });

    const changes = [
        { label: '关闭动画', change: () => preferencesStore.setMotionEnabled(false), loops: false },
        { label: '改变速度', change: () => preferencesStore.setMotionSpeed(1.5), loops: true },
        { label: '改变档位', change: () => preferencesStore.setMotionLevel('rich'), loops: true },
    ];

    it.each(changes)('入场中$label：立即归位，后续不被旧动画改回中间态', ({ change, loops }) => {
        const { wrap, path } = renderEnteringIcon();
        advance(0.02);
        expect(Number(wrap.style.opacity)).toBeLessThan(1);
        expect(wrap.style.transform).not.toBe('');

        act(change);

        expectRestored(wrap, path);
        expect(wrap.classList.contains('ndf-icon-loop')).toBe(loops);
        advance(1);
        expectRestored(wrap, path);

        act(() => preferencesStore.setMotionEnabled(true));
        expectRestored(wrap, path);
        expect(wrap.classList.contains('ndf-icon-loop--pulse')).toBe(true);
    });

    it('进场中卸载会停止动画并释放描边和 transform', () => {
        const { wrap, path, unmount } = renderEnteringIcon();
        advance(0.02);
        unmount();
        advance(1);
        expectRestored(wrap, path);
        expect(gsap.getTweensOf([wrap, path])).toHaveLength(0);
    });

    it('速度加倍时，实际描边动画时长减半', () => {
        const slow = renderEnteringIcon();
        const slowTween = gsap.getTweensOf(slow.path).find((t) => t.vars.strokeDashoffset === 0)!;
        const slowDuration = slowTween.duration();
        slow.unmount();

        act(() => preferencesStore.setMotionSpeed(1));
        const fast = renderEnteringIcon();
        const fastTween = gsap.getTweensOf(fast.path).find((t) => t.vars.strokeDashoffset === 0)!;
        expect(slowDuration / fastTween.duration()).toBeCloseTo(2);
    });
});

describe('MotionIcon 描边量测', () => {
    it('读取所有几何长度时尚未写入动画样式，stroke none 的节点保持静态', () => {
        const readsAfterWrite: boolean[] = [];
        const measure = (node: SVGPathElement | null) => {
            if (!node) return;
            Object.defineProperty(node, 'getTotalLength', {
                value: () => {
                    const svg = node.closest('svg')!;
                    readsAfterWrite.push(
                        Array.from(svg.querySelectorAll('path')).some(
                            (path) => path.style.strokeDasharray !== '',
                        ) || (svg.parentElement as HTMLElement).style.transform !== '',
                    );
                    return 40;
                },
            });
        };
        function ProbeIcon({ size, strokeWidth }: LucideProps) {
            return (
                <svg width={size} height={size} stroke="currentColor" strokeWidth={strokeWidth}>
                    <g>
                        <path ref={measure} d="M0 0 L10 10" />
                        <path ref={measure} d="M10 0 L0 10" />
                        <path ref={measure} stroke="none" d="M0 0 L10 0" />
                    </g>
                </svg>
            );
        }

        const { container } = render(
            <MotionIcon icon={ProbeIcon} motion="pulse" enterKey="probe" />,
        );

        expect(readsAfterWrite).toEqual([false, false]);
        const paths = container.querySelectorAll<SVGPathElement>('path');
        expect(paths[0]!.style.strokeDasharray).toBe('40');
        expect(paths[1]!.style.strokeDasharray).toBe('40');
        expect(paths[2]!.style.strokeDasharray).toBe('');
    });
});

describe('MotionIcon 悬停反馈', () => {
    it('进场期间快速悬停不打断描边或淡入', () => {
        const { container } = render(
            <MotionIcon icon={MessageSquare} motion="pulse" enterKey="chat" hoverAccent />,
        );
        const wrap = container.firstElementChild as HTMLSpanElement;
        const path = wrap.querySelector<SVGPathElement>('path')!;
        pointer(wrap, 'mouseenter');
        advance(0.02);
        pointer(wrap, 'mouseleave');
        pointer(wrap, 'mouseenter');
        advance(1);

        expectRestored(wrap, path);
        expect(wrap.classList.contains('ndf-icon-loop--pulse')).toBe(true);
    });

    it('连续进出只保留当前反馈，离开后归位并释放动画', () => {
        const { container } = render(<MotionIcon icon={MessageSquare} hoverAccent />);
        const wrap = container.firstElementChild as HTMLSpanElement;

        for (let i = 0; i < 12; i += 1) {
            pointer(wrap, 'mouseenter');
            advance(0.02);
            expect(gsap.getTweensOf(wrap).length).toBeLessThanOrEqual(2);
            pointer(wrap, 'mouseleave');
            expect(gsap.getTweensOf(wrap).length).toBeLessThanOrEqual(1);
        }
        advance(1);

        expect(wrap.style.transform).toBe('');
        expect(gsap.getTweensOf(wrap)).toHaveLength(0);
    });

    it('悬停期间关闭反馈不会留下缩放', () => {
        const { container, rerender } = render(<MotionIcon icon={MessageSquare} hoverAccent />);
        const wrap = container.firstElementChild as HTMLSpanElement;
        pointer(wrap, 'mouseenter');
        advance(0.02);
        expect(wrap.style.transform).not.toBe('');

        rerender(<MotionIcon icon={MessageSquare} hoverAccent={false} />);
        advance(1);

        expect(wrap.style.transform).toBe('');
        expect(gsap.getTweensOf(wrap)).toHaveLength(0);
    });

    it('CSS 循环期间悬停不启动竞争动画或移除循环', () => {
        const { container } = render(
            <MotionIcon icon={MessageSquare} motion="pulse" playEnter={false} hoverAccent />,
        );
        const wrap = container.firstElementChild as HTMLSpanElement;
        const duration = wrap.style.getPropertyValue('--ndf-icon-dur');

        for (let i = 0; i < 5; i += 1) {
            pointer(wrap, 'mouseenter');
            pointer(wrap, 'mouseleave');
        }
        advance(1);

        expect(wrap.classList.contains('ndf-icon-loop--pulse')).toBe(true);
        expect(wrap.style.getPropertyValue('--ndf-icon-dur')).toBe(duration);
        expect(wrap.style.transform).toBe('');
        expect(gsap.getTweensOf(wrap)).toHaveLength(0);
    });
});
