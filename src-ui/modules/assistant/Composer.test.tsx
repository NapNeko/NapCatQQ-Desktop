// @vitest-environment jsdom

import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { preferencesStore } from '../../core/domain/settings/preferencesStore';
import { Composer } from './Composer';

beforeEach(() => {
    act(() => {
        preferencesStore.reset();
        preferencesStore.setMotionEnabled(false);
    });
});
afterEach(() => {
    cleanup();
    act(() => preferencesStore.reset());
});

describe('诊断输入框键盘操作', () => {
    it('输入法确认及 Shift+Enter 保留输入，普通 Enter 才提交', () => {
        const onSubmit = vi.fn();
        render(
            <Composer
                label="描述问题"
                placeholder="输入问题"
                value="为什么收不到消息？"
                action={<button type="button">发送</button>}
                onChange={vi.fn()}
                onSubmit={onSubmit}
                autoFocus
            />,
        );
        const input = screen.getByRole('textbox', { name: '描述问题' });
        expect(input).toHaveFocus();
        expect(fireEvent.keyDown(input, { key: 'Enter', isComposing: true })).toBe(true);
        expect(fireEvent.keyDown(input, { key: 'Enter', keyCode: 229 })).toBe(true);
        expect(fireEvent.keyDown(input, { key: 'Enter', shiftKey: true })).toBe(true);
        expect(onSubmit).not.toHaveBeenCalled();
        expect(input).toHaveValue('为什么收不到消息？');
        expect(fireEvent.keyDown(input, { key: 'Enter', keyCode: 13 })).toBe(false);
        expect(onSubmit).toHaveBeenCalledOnce();
    });
});
