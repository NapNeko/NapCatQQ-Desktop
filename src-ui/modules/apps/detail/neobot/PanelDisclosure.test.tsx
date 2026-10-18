import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { preferencesStore } from '../../../../core/domain/settings/preferencesStore';
import { JsonDraftField } from './NeoBotSchemaFields';
import { PanelDisclosure } from './PanelDisclosure';

let motionEnabled: boolean;
beforeEach(() => {
    motionEnabled = preferencesStore.get().motionEnabled;
    preferencesStore.setMotionEnabled(false);
});
afterEach(() => {
    cleanup();
    preferencesStore.setMotionEnabled(motionEnabled);
});

it('收起再展开高级元数据时保留未通过校验的 JSON 草稿', () => {
    const validity = vi.fn();
    render(
        <PanelDisclosure label="高级元数据" defaultOpen>
            <JsonDraftField
                label="元数据"
                value={{ enabled: true }}
                onChange={vi.fn()}
                onValidity={validity}
            />
        </PanelDisclosure>,
    );
    const trigger = screen.getByRole('button', { name: '高级元数据' });
    const input = screen.getByRole('textbox', { name: '元数据' });
    fireEvent.change(input, { target: { value: '{broken' } });
    expect(validity).toHaveBeenLastCalledWith(false);

    fireEvent.click(trigger);
    expect(trigger).toHaveAttribute('aria-expanded', 'false');
    expect(input).toBeInTheDocument();
    expect(screen.queryByRole('textbox', { name: '元数据' })).not.toBeInTheDocument();

    fireEvent.click(trigger);
    expect(trigger).toHaveAttribute('aria-expanded', 'true');
    expect(screen.getByRole('textbox', { name: '元数据' })).toBe(input);
    expect(input).toHaveValue('{broken');
    expect(screen.getByText('JSON 格式不正确')).toBeVisible();
});
