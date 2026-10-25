// @vitest-environment jsdom

import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { AssistantSessionInfo } from '../../core/domain/assistant/types';
import { preferencesStore } from '../../core/domain/settings/preferencesStore';
import { MiniMaxLogin } from './MiniMaxLogin';

const login = vi.hoisted(() => ({
    start: vi.fn(),
    busy: false,
    error: null as string | null,
    feedback: null as string | null,
    terminalEnabled: true,
    preview: false,
    command: null as string | null,
    shellLabel: null as string | null,
}));
vi.mock('../../hooks/assistant/useAssistantLogin', () => ({ useAssistantLogin: () => login }));

function info(
    state: AssistantSessionInfo['state'] = 'ready',
    provider: AssistantSessionInfo['provider'] = 'miniMax',
): AssistantSessionInfo {
    return {
        id: 'session-1',
        provider,
        botId: '10001',
        botName: '测试 Bot',
        title: '诊断',
        state,
        createdAtMs: 100,
        updatedAtMs: 100,
        error: null,
        capabilities: {
            mcpHttp: true,
            loadSession: true,
            resumeSession: true,
            images: false,
            elicitation: true,
        },
    };
}

beforeEach(() => {
    Object.assign(login, {
        busy: false,
        error: null,
        feedback: null,
        terminalEnabled: true,
        preview: false,
        command: null,
        shellLabel: null,
    });
    login.start.mockReset();
    act(() => {
        preferencesStore.reset();
        preferencesStore.setMotionEnabled(false);
    });
});
afterEach(() => {
    cleanup();
    act(() => preferencesStore.reset());
});

describe('MiniMax 登录入口', () => {
    it.each(['ready', 'failed', 'disconnected', 'closed'] as const)(
        '%s 的 MiniMax 会话提供显式登录按钮',
        (state) => {
            render(<MiniMaxLogin info={info(state)} />);
            expect(login.start).not.toHaveBeenCalled();
            fireEvent.click(screen.getByRole('button', { name: '登录 MiniMax' }));
            expect(login.start).toHaveBeenCalledOnce();
        },
    );

    it.each(['starting', 'running', 'waitingForInput'] as const)('%s 时不允许另启登录', (state) => {
        render(<MiniMaxLogin info={info(state)} />);
        expect(screen.queryByRole('region', { name: 'MiniMax 登录' })).not.toBeInTheDocument();
        expect(login.start).not.toHaveBeenCalled();
    });

    it('其他 Agent 不显示 MiniMax 登录入口', () => {
        render(<MiniMaxLogin info={info('ready', 'qwen')} />);
        expect(screen.queryByRole('button')).not.toBeInTheDocument();
    });

    it('终端关闭时清楚提供复制方式与指定 shell 的手动命令', () => {
        Object.assign(login, {
            terminalEnabled: false,
            command: 'safe-login --home "session-dir"',
            shellLabel: 'Windows PowerShell',
        });
        render(<MiniMaxLogin info={info()} />);
        expect(screen.getByText(/内嵌终端已关闭/)).toBeVisible();
        expect(screen.getByText('手动运行命令（Windows PowerShell）')).toBeVisible();
        fireEvent.click(screen.getByRole('button', { name: '复制 MiniMax 登录命令' }));
        expect(login.start).toHaveBeenCalledOnce();
    });

    it('浏览器预览明确不执行登录，按钮禁用', () => {
        login.preview = true;
        render(<MiniMaxLogin info={info()} />);
        expect(screen.getByText('浏览器预览不执行登录。')).toBeVisible();
        const button = screen.getByRole('button', { name: '登录 MiniMax' });
        expect(button).toBeDisabled();
        fireEvent.click(button);
        expect(login.start).not.toHaveBeenCalled();
    });
});
