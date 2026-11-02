// @vitest-environment jsdom
import { fireEvent, render, screen, cleanup } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { Tabs, TooltipProvider } from '../../../shared/ui';
import type { AppInstance } from '../../../core/ipc/types';
import { DetailHeader } from './DetailHeader';
import { DetailSideNav } from './DetailSideNav';

vi.mock('../../../hooks/preferences/featureTogglesStore', () => ({
    useFeatureEnabled: () => true,
}));
vi.mock('./WebUiAccountCard', () => ({ WebUiAccountCard: () => <div>WebUI 账号</div> }));

const instance: AppInstance = {
    id: 'app-1',
    framework_id: 'karin',
    display_name: 'Karin',
    host_id: 'local',
    placement: 'local_native',
    install_dir: '/d/apps/karin',
    port: 7777,
    state: 'running',
    origin: 'created',
    auto_start: false,
    install_renderer: false,
    created_at_ms: 1,
    installed_version: '1.0.0',
};

afterEach(cleanup);

describe('应用实例管理入口', () => {
    it('详情导航提供配置、日志和可选的版本', () => {
        const view = render(
            <Tabs defaultValue="config">
                <DetailSideNav withVersionTab />
            </Tabs>,
        );
        expect(screen.getAllByRole('tab').map((tab) => tab.textContent)).toEqual([
            '配置',
            '日志',
            '版本',
        ]);
        view.rerender(
            <Tabs defaultValue="config">
                <DetailSideNav withVersionTab={false} />
            </Tabs>,
        );
        expect(screen.getAllByRole('tab').map((tab) => tab.textContent)).toEqual(['配置', '日志']);
    });

    it('运行时可直接打开 WebUI 和一键对接，停止后保留启动入口', () => {
        const onWebUi = vi.fn();
        const onLink = vi.fn();
        const onStart = vi.fn();
        const props = {
            instance,
            hostLabel: '本机',
            installed: true,
            busy: false,
            canWebUi: true,
            onWebUi,
            onLink,
            onStart,
            onBack: vi.fn(),
            onInstall: vi.fn(),
            onStop: vi.fn(),
            onUnlink: vi.fn(),
            onRefresh: vi.fn(),
            onDelete: vi.fn(),
            onAutoStartChange: vi.fn(),
        };
        const view = render(
            <TooltipProvider>
                <DetailHeader {...props} />
            </TooltipProvider>,
        );
        fireEvent.click(screen.getByRole('button', { name: '打开 WebUI' }));
        fireEvent.click(screen.getByRole('button', { name: '一键对接' }));
        expect(onWebUi).toHaveBeenCalledOnce();
        expect(onLink).toHaveBeenCalledOnce();
        view.rerender(
            <TooltipProvider>
                <DetailHeader
                    {...props}
                    instance={{ ...instance, state: 'stopped' }}
                    canWebUi={false}
                />
            </TooltipProvider>,
        );
        expect(screen.queryByRole('button', { name: '打开 WebUI' })).toBeNull();
        fireEvent.click(screen.getByRole('button', { name: '启动' }));
        expect(onStart).toHaveBeenCalledOnce();
    });
});
