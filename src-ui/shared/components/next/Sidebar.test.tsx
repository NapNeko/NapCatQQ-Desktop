import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { Sidebar } from './Sidebar';

function renderSidebar(sidebarStyle?: 'classic' | 'floating', collapsed = false) {
    return render(
        <Sidebar
            active="overview"
            onChange={() => {}}
            collapsed={collapsed}
            sidebarStyle={sidebarStyle}
            onToggleCollapse={vi.fn()}
        />,
    );
}

describe('Sidebar 形态切换', () => {
    it('悬浮窄栏无折叠按钮且导航 label 隐藏（图标态）', () => {
        renderSidebar('floating', false);
        expect(screen.queryByRole('button', { name: '展开侧栏' })).toBeNull();
        expect(screen.queryByRole('button', { name: '折叠侧栏' })).toBeNull();
        expect(screen.queryByText('机器人')).toBeNull();
        expect(screen.getByTitle('概览')).toBeTruthy();
    });

    it('经典贴边档展开态保留折叠按钮与 label', () => {
        renderSidebar('classic', false);
        expect(screen.getByRole('button', { name: '折叠侧栏' })).toBeTruthy();
        expect(screen.getByText('机器人')).toBeTruthy();
    });

    it('经典档折叠态隐藏 label 并给展开按钮', () => {
        renderSidebar('classic', true);
        expect(screen.getByRole('button', { name: '展开侧栏' })).toBeTruthy();
        expect(screen.queryByText('机器人')).toBeNull();
    });

    it('缺省按悬浮窄栏渲染', () => {
        renderSidebar(undefined, false);
        expect(screen.queryByRole('button', { name: '折叠侧栏' })).toBeNull();
        expect(screen.queryByText('机器人')).toBeNull();
    });
});
