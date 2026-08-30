import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it } from 'vitest';
import { preferencesStore } from '../../hooks/preferences/preferencesStore';
import {
    ContextMenu,
    ContextMenuContent,
    ContextMenuItem,
    ContextMenuTrigger,
} from './ContextMenu';

describe('ContextMenu 动效与档位', () => {
    afterEach(() => {
        preferencesStore.reset();
    });

    it('默认 standard 档位下注入标准进退场动画与 CSS 变量', async () => {
        const user = userEvent.setup();
        preferencesStore.setMotionLevel('standard');
        preferencesStore.setMotionSpeed(1.0);

        render(
            <ContextMenu>
                <ContextMenuTrigger>右键目标</ContextMenuTrigger>
                <ContextMenuContent>
                    <ContextMenuItem>操作选项</ContextMenuItem>
                </ContextMenuContent>
            </ContextMenu>,
        );

        await user.pointer({ keys: '[MouseRight]', target: screen.getByText('右键目标') });

        const menu = await screen.findByRole('menu');
        expect(menu).toHaveClass('ndf-context-menu-content');
        expect(menu.style.getPropertyValue('--cm-anim-in-name')).toBe('context-menu-in');
        expect(menu.style.getPropertyValue('--cm-scale-from')).toBe('0.92');
        expect(menu.style.getPropertyValue('--cm-scale-to')).toBe('0.95');
        expect(menu.style.getPropertyValue('--cm-duration-in')).toBe('140ms');
    });

    it('rich 档位下注入弹性 rich 动画与放大的位移幅度', async () => {
        const user = userEvent.setup();
        preferencesStore.setMotionLevel('rich');
        preferencesStore.setMotionSpeed(1.0);

        render(
            <ContextMenu>
                <ContextMenuTrigger>右键目标</ContextMenuTrigger>
                <ContextMenuContent>
                    <ContextMenuItem>操作选项</ContextMenuItem>
                </ContextMenuContent>
            </ContextMenu>,
        );

        await user.pointer({ keys: '[MouseRight]', target: screen.getByText('右键目标') });

        const menu = await screen.findByRole('menu');
        expect(menu).toHaveClass('ndf-context-menu-content');
        expect(menu.style.getPropertyValue('--cm-anim-in-name')).toBe('context-menu-in-rich');
        expect(menu.style.getPropertyValue('--cm-scale-from')).toBe('0.88');
        expect(menu.style.getPropertyValue('--cm-scale-to')).toBe('0.92');
        expect(menu.style.getPropertyValue('--cm-duration-in')).toBe('240ms');
    });

    it('elegant 档位下注入微缩放优雅平滑动画', async () => {
        const user = userEvent.setup();
        preferencesStore.setMotionLevel('elegant');
        preferencesStore.setMotionSpeed(1.0);

        render(
            <ContextMenu>
                <ContextMenuTrigger>右键目标</ContextMenuTrigger>
                <ContextMenuContent>
                    <ContextMenuItem>操作选项</ContextMenuItem>
                </ContextMenuContent>
            </ContextMenu>,
        );

        await user.pointer({ keys: '[MouseRight]', target: screen.getByText('右键目标') });

        const menu = await screen.findByRole('menu');
        expect(menu).toHaveClass('ndf-context-menu-content');
        expect(menu.style.getPropertyValue('--cm-anim-in-name')).toBe('context-menu-in');
        expect(menu.style.getPropertyValue('--cm-scale-from')).toBe('0.96');
        expect(menu.style.getPropertyValue('--cm-scale-to')).toBe('0.98');
        expect(menu.style.getPropertyValue('--cm-duration-in')).toBe('120ms');
    });

    it('当 motionEnabled 为 false 时禁用动画并设 animation 为 none', async () => {
        const user = userEvent.setup();
        preferencesStore.setMotionEnabled(false);

        render(
            <ContextMenu>
                <ContextMenuTrigger>右键目标</ContextMenuTrigger>
                <ContextMenuContent>
                    <ContextMenuItem>操作选项</ContextMenuItem>
                </ContextMenuContent>
            </ContextMenu>,
        );

        await user.pointer({ keys: '[MouseRight]', target: screen.getByText('右键目标') });

        const menu = await screen.findByRole('menu');
        expect(menu.style.animation).toBe('none');
    });
});
