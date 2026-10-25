// @vitest-environment jsdom

import { useState } from 'react';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { preferencesStore } from '../../../core/domain/settings/preferencesStore';
import { mockBackendSettings } from '../../../core/ipc/mock/settings.mock';
import { draftFromBackendAndPrefs, type SettingsDraft } from '../settings-draft';
import { McpSection } from './McpSection';

const state = vi.hoisted(() => ({
    status: { enabled: true, listening: true, port: 4567, lastError: null as string | null },
    error: null as string | null,
    busy: false,
    feedback: null as string | null,
    copy: vi.fn(),
    rotate: vi.fn(),
}));
vi.mock('../../../hooks/settings/useMcpSettings', () => ({ useMcpSettings: () => state }));

beforeAll(() => {
    Element.prototype.hasPointerCapture ??= () => false;
    Element.prototype.releasePointerCapture ??= () => {};
    Element.prototype.scrollIntoView ??= () => {};
});

function Form({ enabled = false, dangerous = false }: { enabled?: boolean; dangerous?: boolean }) {
    const [draft, setDraft] = useState(() => ({
        ...draftFromBackendAndPrefs(mockBackendSettings),
        mcp: { enabled, port: 0, readOnly: !dangerous, allowDangerous: dangerous },
    }));
    return (
        <McpSection
            draft={draft}
            patchDraft={(patch: Partial<SettingsDraft>) =>
                setDraft((current) => ({ ...current, ...patch }))
            }
        />
    );
}

beforeEach(() => {
    act(() => {
        preferencesStore.reset();
        preferencesStore.setMotionEnabled(false);
    });
    state.status.lastError = null;
    state.feedback = null;
    state.error = null;
    state.copy.mockClear();
    state.rotate.mockClear();
});
afterEach(() => {
    cleanup();
    act(() => preferencesStore.reset());
});

describe('MCP 设置', () => {
    it('关闭时只保留启用入口，启用后先展示连接操作', () => {
        render(<Form />);
        expect(screen.queryByRole('button', { name: '复制连接配置' })).not.toBeInTheDocument();
        expect(screen.queryByRole('spinbutton')).not.toBeInTheDocument();
        fireEvent.click(screen.getByRole('switch', { name: '启用 MCP 服务' }));
        expect(screen.getByRole('button', { name: '复制连接配置' })).toBeEnabled();
        expect(screen.queryByRole('combobox', { name: 'MCP 访问权限' })).not.toBeInTheDocument();
        fireEvent.click(screen.getByRole('button', { name: /高级设置/ }));
        expect(screen.getByRole('combobox', { name: 'MCP 访问权限' })).toBeVisible();
    });

    it('切回只读时一并撤销危险权限', async () => {
        render(<Form enabled dangerous />);
        fireEvent.click(screen.getByRole('button', { name: /高级设置/ }));
        const user = userEvent.setup();
        await user.click(screen.getByRole('combobox', { name: 'MCP 访问权限' }));
        await user.click(screen.getByRole('option', { name: '只读' }));
        expect(screen.getByRole('button', { name: /高级设置.*只读访问/ })).toBeVisible();
        expect(screen.queryByText(/允许撤回、踢人/)).not.toBeInTheDocument();
        await user.click(screen.getByRole('combobox', { name: 'MCP 访问权限' }));
        await user.click(screen.getByRole('option', { name: '允许写入' }));
        expect(screen.getByRole('button', { name: /高级设置.*允许写入/ })).toBeVisible();
    });

    it('令牌重置在说明旧配置失效后确认执行', () => {
        render(<Form enabled />);
        fireEvent.click(screen.getByRole('button', { name: /高级设置/ }));
        fireEvent.click(screen.getByRole('button', { name: '重置令牌…' }));
        expect(state.rotate).not.toHaveBeenCalled();
        expect(screen.getByText(/旧连接配置将立即失效/)).toBeVisible();
        fireEvent.click(screen.getByRole('button', { name: '重置访问令牌' }));
        expect(state.rotate).toHaveBeenCalledOnce();
    });

    it('先前的复制反馈不会掩盖当前服务故障', () => {
        state.feedback = '连接配置已复制';
        state.status.lastError = '端口被占用';
        render(<Form enabled />);
        expect(screen.getByRole('alert')).toHaveTextContent('端口被占用');
        expect(screen.queryByText('连接配置已复制')).not.toBeInTheDocument();
    });
});
