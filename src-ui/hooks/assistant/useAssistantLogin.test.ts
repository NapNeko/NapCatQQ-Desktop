// @vitest-environment jsdom

import { act, cleanup, renderHook, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { AssistantLoginPlan } from '../../core/domain/assistant/types';
import { featureTogglesStore } from '../preferences/featureTogglesStore';
import { useAssistantLogin } from './useAssistantLogin';

const service = vi.hoisted(() => ({ preview: false, loginPlan: vi.fn() }));
const terminal = vi.hoisted(() => ({ open: vi.fn(), write: vi.fn() }));
const clipboard = vi.hoisted(() => ({ writeText: vi.fn() }));
vi.mock('../../core/services/assistant.service', () => ({ assistantService: service }));
vi.mock('../terminal/terminalStore', () => ({ terminalStore: { open: terminal.open } }));
vi.mock('../../core/services/terminal.service', () => ({
    terminalService: { write: terminal.write },
}));
vi.mock('../../core/services/clipboard.service', () => ({ clipboardService: clipboard }));

const plan: AssistantLoginPlan = {
    command: 'safe-login --home "session-dir"',
    shell: 'windows_powershell',
};

beforeEach(() => {
    service.preview = false;
    service.loginPlan.mockReset().mockResolvedValue(plan);
    terminal.open.mockReset().mockResolvedValue('terminal-1');
    terminal.write.mockReset().mockResolvedValue(undefined);
    clipboard.writeText.mockReset().mockResolvedValue(undefined);
    act(() => {
        featureTogglesStore._reset();
        featureTogglesStore.apply({ terminal: true });
    });
});
afterEach(() => {
    cleanup();
    act(() => featureTogglesStore._reset());
});

describe('MiniMax 显式登录', () => {
    it('挂载时不启动认证，点击后才创建指定 shell 的新可见终端并提交固定命令', async () => {
        const { result } = renderHook(() => useAssistantLogin('session-1'));
        expect(service.loginPlan).not.toHaveBeenCalled();
        expect(terminal.open).not.toHaveBeenCalled();
        await act(async () => {
            await result.current.start();
        });
        expect(service.loginPlan).toHaveBeenCalledWith('session-1');
        expect(terminal.open).toHaveBeenCalledWith(
            { kind: 'local' },
            { shell: 'windows_powershell', forceNew: true },
        );
        expect(terminal.write).toHaveBeenCalledWith('terminal-1', `${plan.command}\r`);
        expect(clipboard.writeText).not.toHaveBeenCalled();
        expect(result.current.feedback).toContain('登录命令已提交到终端');
    });

    it('终端功能关闭时只复制命令，不启动隐藏认证过程', async () => {
        act(() => featureTogglesStore.apply({ terminal: false }));
        const { result } = renderHook(() => useAssistantLogin('session-1'));
        await act(async () => {
            await result.current.start();
        });
        expect(terminal.open).not.toHaveBeenCalled();
        expect(terminal.write).not.toHaveBeenCalled();
        expect(clipboard.writeText).toHaveBeenCalledWith(plan.command);
        expect(result.current.command).toBe(plan.command);
        expect(result.current.feedback).toContain('Windows PowerShell');
    });

    it('等待登录方案时关闭终端功能，会转为复制而不创建终端', async () => {
        let resolve!: (value: AssistantLoginPlan) => void;
        service.loginPlan.mockReturnValueOnce(
            new Promise<AssistantLoginPlan>((done) => {
                resolve = done;
            }),
        );
        const { result } = renderHook(() => useAssistantLogin('session-1'));
        let work!: Promise<void>;
        act(() => {
            work = result.current.start();
        });
        act(() => featureTogglesStore.apply({ terminal: false }));
        await act(async () => {
            resolve(plan);
            await work;
        });
        expect(terminal.open).not.toHaveBeenCalled();
        expect(clipboard.writeText).toHaveBeenCalledWith(plan.command);
    });

    it('等待终端打开时关闭功能，也不能继续写入认证命令', async () => {
        let resolve!: (value: string) => void;
        terminal.open.mockReturnValueOnce(
            new Promise<string>((done) => {
                resolve = done;
            }),
        );
        const { result } = renderHook(() => useAssistantLogin('session-1'));
        let work!: Promise<void>;
        act(() => {
            work = result.current.start();
        });
        await waitFor(() => expect(terminal.open).toHaveBeenCalledOnce());
        act(() => featureTogglesStore.apply({ terminal: false }));
        await act(async () => {
            resolve('terminal-1');
            await work;
        });
        expect(terminal.write).not.toHaveBeenCalled();
        expect(clipboard.writeText).toHaveBeenCalledWith(plan.command);
    });

    it('浏览器预览不请求方案、不打开终端，也不复制或执行命令', async () => {
        service.preview = true;
        const { result } = renderHook(() => useAssistantLogin('session-1'));
        await act(async () => {
            await result.current.start();
        });
        expect(service.loginPlan).not.toHaveBeenCalled();
        expect(terminal.open).not.toHaveBeenCalled();
        expect(terminal.write).not.toHaveBeenCalled();
        expect(clipboard.writeText).not.toHaveBeenCalled();
        expect(result.current.feedback).toContain('浏览器预览不执行登录');
    });

    it('方案错误保留原因并释放忙状态，用户可以明确重试', async () => {
        service.loginPlan.mockRejectedValueOnce('登录配置不可用');
        const { result } = renderHook(() => useAssistantLogin('session-1'));
        await act(async () => {
            await result.current.start();
        });
        expect(result.current.error).toBe('登录配置不可用');
        expect(result.current.busy).toBe(false);
        expect(terminal.open).not.toHaveBeenCalled();
        await act(async () => {
            await result.current.start();
        });
        expect(service.loginPlan).toHaveBeenCalledTimes(2);
        expect(terminal.write).toHaveBeenCalledOnce();
    });

    it('终端无法创建时不写命令，保留命令供用户手动运行', async () => {
        terminal.open.mockResolvedValueOnce(null);
        const { result } = renderHook(() => useAssistantLogin('session-1'));
        await act(async () => {
            await result.current.start();
        });
        expect(terminal.write).not.toHaveBeenCalled();
        expect(result.current.error).toContain('本机终端未能打开');
        expect(result.current.command).toBe(plan.command);
        expect(result.current.busy).toBe(false);
    });
});
