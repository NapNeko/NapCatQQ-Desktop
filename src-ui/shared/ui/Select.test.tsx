import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import gsap from 'gsap';
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import { preferencesStore } from '../../hooks/preferences/preferencesStore';
import { Select } from './Select';

const ITEMS = [
    { value: 'a', label: '甲' },
    { value: 'b', label: '乙' },
];

function renderSelect() {
    return render(<Select items={ITEMS} value="a" onValueChange={() => { }} />);
}

// Radix Select 打开时会用到这几个 jsdom 没有的 DOM API
beforeAll(() => {
    Element.prototype.hasPointerCapture ??= () => false;
    Element.prototype.releasePointerCapture ??= () => { };
    Element.prototype.scrollIntoView ??= () => { };
});

afterEach(() => {
    preferencesStore.reset();
    vi.restoreAllMocks();
});

describe('Select 弹出层进场', () => {
    it('一次打开只进场一次：Radix 内部重渲、父级重渲都不重播', async () => {
        const user = userEvent.setup();
        const fromTo = vi.spyOn(gsap, 'fromTo');
        const { rerender } = renderSelect();

        await user.click(screen.getByRole('combobox'));
        const listbox = await screen.findByRole('listbox');
        rerender(<Select items={ITEMS} value="a" onValueChange={() => { }} />);

        expect(fromTo.mock.calls.filter(([target]) => target === listbox)).toHaveLength(1);
    });

    it('关掉再打开是新的内容节点，照常进场', async () => {
        const user = userEvent.setup();
        const fromTo = vi.spyOn(gsap, 'fromTo');
        renderSelect();

        await user.click(screen.getByRole('combobox'));
        const first = await screen.findByRole('listbox');
        await user.keyboard('{Escape}');
        await user.click(screen.getByRole('combobox'));
        const second = await screen.findByRole('listbox');

        expect(second).not.toBe(first);
        expect(fromTo.mock.calls.filter(([target]) => target === second)).toHaveLength(1);
    });

    it('关掉动效时直接可见，不跑进场动画', async () => {
        const user = userEvent.setup();
        preferencesStore.setMotionEnabled(false);
        const fromTo = vi.spyOn(gsap, 'fromTo');
        renderSelect();

        await user.click(screen.getByRole('combobox'));
        const listbox = await screen.findByRole('listbox');

        expect(listbox.style.visibility).not.toBe('hidden');
        expect(fromTo.mock.calls.filter(([target]) => target === listbox)).toHaveLength(0);
    });
});
