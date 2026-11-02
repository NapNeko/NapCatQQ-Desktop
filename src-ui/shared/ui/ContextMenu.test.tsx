import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { preferencesStore } from '../../core/domain/settings/preferencesStore';
import {
    ContextMenu,
    ContextMenuContent,
    ContextMenuItem,
    ContextMenuTrigger,
} from './ContextMenu';

describe('ContextMenu', () => {
    afterEach(() => {
        preferencesStore.reset();
    });

    it('关闭动画后仍能打开菜单并执行选中的操作', async () => {
        const user = userEvent.setup();
        const onSelect = vi.fn();
        preferencesStore.setMotionEnabled(false);

        render(
            <ContextMenu>
                <ContextMenuTrigger>右键目标</ContextMenuTrigger>
                <ContextMenuContent>
                    <ContextMenuItem onSelect={onSelect}>操作选项</ContextMenuItem>
                </ContextMenuContent>
            </ContextMenu>,
        );

        await user.pointer({ keys: '[MouseRight]', target: screen.getByText('右键目标') });

        const menu = await screen.findByRole('menu');
        expect(menu.style.animation).toBe('none');
        await user.click(screen.getByRole('menuitem', { name: '操作选项' }));
        expect(onSelect).toHaveBeenCalledOnce();
    });
});
