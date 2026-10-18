import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterAll, afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import { preferencesStore } from '../../../core/domain/settings/preferencesStore';
import type { DebugActionSpec } from '../../../core/ipc/generated/debug/DebugActionSpec';
import { DocsPane } from './DocsPane';

const originalHeight = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'offsetHeight');
const originalWidth = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'offsetWidth');

beforeAll(() => {
    Object.defineProperty(HTMLElement.prototype, 'offsetHeight', {
        configurable: true,
        get() {
            return this.getAttribute('role') === 'tree' ? 320 : 22;
        },
    });
    Object.defineProperty(HTMLElement.prototype, 'offsetWidth', {
        configurable: true,
        get: () => 600,
    });
});

afterAll(() => {
    if (originalHeight)
        Object.defineProperty(HTMLElement.prototype, 'offsetHeight', originalHeight);
    else Reflect.deleteProperty(HTMLElement.prototype, 'offsetHeight');
    if (originalWidth) Object.defineProperty(HTMLElement.prototype, 'offsetWidth', originalWidth);
    else Reflect.deleteProperty(HTMLElement.prototype, 'offsetWidth');
});

afterEach(() => {
    cleanup();
    preferencesStore.reset();
});

const spec: DebugActionSpec = {
    name: 'get_status',
    aliases: [],
    summary: '读取状态',
    description: null,
    category: 'account',
    safety: 'read_only',
    stream: false,
    supported: true,
    params_schema: { type: 'object', properties: {} },
    returns_schema: null,
    returns_text: null,
    return_example: { outer: { inner: { value: 1 } } },
    examples: [],
    error_examples: [],
    invariants: [],
    other_backend: null,
    source: 'live',
};

describe('返回示例折叠面板', () => {
    it('支持键盘开合并保留 JSON 树内部展开状态', async () => {
        preferencesStore.setMotionEnabled(false);
        const user = userEvent.setup();
        const onUseExample = vi.fn();
        render(<DocsPane spec={spec} backend="napcat" dirty onUseExample={onUseExample} />);
        const trigger = screen.getByRole('button', { name: '返回示例' });
        expect(trigger).toHaveAttribute('aria-expanded', 'false');
        expect(screen.queryByRole('tree')).not.toBeInTheDocument();

        trigger.focus();
        await user.keyboard('{Enter}');
        const tree = screen.getByRole('tree');
        expect(document.getElementById(trigger.getAttribute('aria-controls')!)).toContainElement(
            tree,
        );
        const inner = screen
            .getAllByRole('treeitem')
            .find((row) => row.textContent?.startsWith('inner:'))!;
        expect(inner).toHaveAttribute('aria-expanded', 'false');
        await user.click(inner);
        expect(inner).toHaveAttribute('aria-expanded', 'true');

        await user.click(trigger);
        expect(trigger).toHaveAttribute('aria-expanded', 'false');
        expect(tree).toBeInTheDocument();
        expect(tree.closest('[inert]')).not.toBeNull();
        expect(screen.queryByRole('tree')).not.toBeInTheDocument();

        await user.keyboard(' ');
        expect(trigger).toHaveAttribute('aria-expanded', 'true');
        expect(screen.getByRole('tree')).toBe(tree);
        expect(inner).toHaveAttribute('aria-expanded', 'true');
        expect(onUseExample).not.toHaveBeenCalled();
    });
});
