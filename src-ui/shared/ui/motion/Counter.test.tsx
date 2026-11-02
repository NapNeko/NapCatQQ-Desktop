import { act, cleanup, render } from '@testing-library/react';
import gsap from 'gsap';
import { afterEach, beforeEach, expect, it } from 'vitest';
import { preferencesStore } from '../../../core/domain/settings/preferencesStore';
import { Counter } from './Counter';

beforeEach(() => {
    gsap.ticker.remove(gsap.updateRoot);
    preferencesStore.reset();
    preferencesStore.setMotionLevel('standard');
    preferencesStore.setMotionEnabled(true);
});

afterEach(() => {
    cleanup();
    preferencesStore.reset();
    gsap.ticker.add(gsap.updateRoot);
});

it('快速更新只保留一轮数字切换，卸载后释放动画与旧数字', () => {
    const { container, rerender, unmount } = render(<Counter value={1} />);
    rerender(<Counter value={2} />);
    const firstOld = container.querySelector('[data-digit="old"]')!;
    const digit = container.querySelector('[data-digit="current"]')!;
    expect(firstOld.textContent).toBe('1');
    expect(gsap.getTweensOf(digit)).toHaveLength(1);

    rerender(<Counter value={3} />);
    const secondOld = container.querySelector('[data-digit="old"]')!;
    expect(container.querySelectorAll('[data-digit="old"]')).toHaveLength(1);
    expect(secondOld.textContent).toBe('2');
    expect(digit.textContent).toBe('3');
    expect(gsap.getTweensOf(firstOld)).toHaveLength(0);
    expect(gsap.getTweensOf(digit)).toHaveLength(1);

    unmount();
    expect(gsap.getTweensOf([digit, firstOld, secondOld])).toHaveLength(0);
    expect(secondOld.parentNode).toBeNull();
});

it('滚动中关闭动画立即显示当前值，旧动画不能再覆盖它', () => {
    const { container, rerender } = render(<Counter value={1} />);
    rerender(<Counter value={2} />);
    const animated = [...container.querySelectorAll('[data-digit]')];
    act(() => preferencesStore.setMotionEnabled(false));
    expect(container.textContent).toBe('2');
    expect(container.querySelector('[data-digit="old"]')).toBeNull();
    expect(gsap.getTweensOf(animated)).toHaveLength(0);
});
