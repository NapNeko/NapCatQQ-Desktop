// @vitest-environment jsdom

import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { AssistantSession } from '../../core/domain/assistant/types';
import { preferencesStore } from '../../core/domain/settings/preferencesStore';
import { Conversation } from './Conversation';

const actions = vi.hoisted(() => ({
    send: vi.fn(),
    cancel: vi.fn(),
    permission: vi.fn(),
    question: vi.fn(),
    draft: vi.fn(),
}));
vi.mock('../../hooks/assistant/assistantStore', () => ({ assistantStore: actions }));
vi.mock('../../hooks/useOpenExternal', () => ({ useOpenExternal: () => vi.fn() }));

function session(state: AssistantSession['info']['state'] = 'ready'): AssistantSession {
    return {
        sequence: 1,
        info: {
            id: 'session-1',
            provider: 'dsh',
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
        },
        messages: [],
        tools: [],
        permissions: [],
        questions: [],
    };
}

beforeEach(() => {
    Object.values(actions).forEach((mock) => mock.mockReset());
    act(() => {
        preferencesStore.reset();
        preferencesStore.setMotionEnabled(false);
    });
});
afterEach(() => {
    cleanup();
    act(() => preferencesStore.reset());
});

describe('诊断会话交互边界', () => {
    it('浏览历史后发送消息会重新跟随最新内容', () => {
        render(
            <Conversation
                session={session()}
                drafts={{ 'session-1': '继续检查' }}
                pending={[]}
                onNew={vi.fn()}
            />,
        );
        const timeline = screen.getByRole('region', { name: '诊断对话' });
        Object.defineProperties(timeline, {
            scrollHeight: { configurable: true, value: 1000 },
            clientHeight: { configurable: true, value: 300 },
        });
        timeline.scrollTop = 100;
        fireEvent.scroll(timeline);
        expect(screen.getByRole('button', { name: '滚动到最新消息' })).toBeVisible();
        fireEvent.click(screen.getByRole('button', { name: '发送' }));
        expect(actions.send).toHaveBeenCalledWith('session-1', '继续检查');
        expect(timeline.scrollTop).toBe(1000);
        expect(screen.queryByRole('button', { name: '滚动到最新消息' })).not.toBeInTheDocument();
    });

    it.each(['failed', 'disconnected'] as const)(
        '%s 由用户发送新消息恢复同一会话，不自动重放旧问题',
        (state) => {
            const current = session(state);
            current.messages.push({
                id: 'old',
                role: 'user',
                text: '上次的操作',
                createdAtMs: 101,
            });
            render(
                <Conversation
                    session={current}
                    drafts={{ 'session-1': '现在是什么状态？' }}
                    pending={[]}
                    onNew={vi.fn()}
                />,
            );
            expect(actions.send).not.toHaveBeenCalled();
            fireEvent.click(screen.getByRole('button', { name: '恢复并发送' }));
            expect(actions.send).toHaveBeenCalledWith('session-1', '现在是什么状态？');
            expect(actions.send).toHaveBeenCalledTimes(1);
        },
    );

    it('关闭的会话保留消息，提供新建入口且不允许直接发送', () => {
        const current = session('closed');
        current.messages.push({
            id: 'reply',
            role: 'assistant',
            text: '已完成诊断',
            createdAtMs: 101,
        });
        const onNew = vi.fn();
        render(
            <Conversation
                session={current}
                drafts={{ 'session-1': '未发出的草稿' }}
                pending={[]}
                onNew={onNew}
            />,
        );
        expect(screen.getByText('已完成诊断')).toBeVisible();
        expect(screen.queryByRole('textbox', { name: '输入诊断问题' })).not.toBeInTheDocument();
        expect(screen.queryByRole('button', { name: '恢复并发送' })).not.toBeInTheDocument();
        fireEvent.click(screen.getByRole('button', { name: '新建会话' }));
        expect(onNew).toHaveBeenCalledOnce();
        expect(actions.send).not.toHaveBeenCalled();
    });

    it('运行时停止当前会话，输入草稿不会被提交', () => {
        render(
            <Conversation
                session={session('running')}
                drafts={{ 'session-1': '稍后再问' }}
                pending={[]}
                onNew={vi.fn()}
            />,
        );
        const input = screen.getByRole('textbox', { name: '输入诊断问题' });
        fireEvent.keyDown(input, { key: 'Enter', code: 'Enter' });
        expect(actions.send).not.toHaveBeenCalled();
        fireEvent.click(screen.getByRole('button', { name: '停止' }));
        expect(actions.cancel).toHaveBeenCalledWith('session-1');
        expect(input).toHaveValue('稍后再问');
    });

    it('桌面审批只传递用户选中的一次性批准选项', () => {
        const current = session('waitingForInput');
        current.permissions.push({
            id: 'approval-1',
            source: 'ncdOperation',
            title: '重启测试 Bot',
            details: '目标：10001\n操作：restart_bot',
            createdAtMs: 101,
            options: [
                { id: 'allow_once', label: '仅允许这一次', kind: 'allow_once' },
                { id: 'reject', label: '拒绝', kind: 'reject_once' },
            ],
        });
        render(<Conversation session={current} drafts={{}} pending={[]} onNew={vi.fn()} />);
        expect(actions.permission).not.toHaveBeenCalled();
        fireEvent.click(screen.getByRole('button', { name: '仅允许这一次' }));
        expect(actions.permission).toHaveBeenCalledWith('session-1', 'approval-1', 'allow_once');
        expect(actions.send).not.toHaveBeenCalled();
    });

    it('追问提交原请求的字段值，跳过时发送取消答案', () => {
        const current = session('waitingForInput');
        current.questions.push({
            id: 'question-1',
            title: '出现时间？',
            createdAtMs: 101,
            fields: [
                {
                    name: 'when',
                    title: '时间',
                    kind: 'string',
                    description: null,
                    required: true,
                    options: [],
                },
            ],
        });
        render(
            <Conversation
                session={current}
                drafts={{ 'question:session-1:question-1': '{"when":"更新之后"}' }}
                pending={[]}
                onNew={vi.fn()}
            />,
        );
        fireEvent.click(screen.getByRole('button', { name: '提交' }));
        expect(actions.question).toHaveBeenCalledWith('session-1', 'question-1', {
            cancelled: false,
            values: { when: '更新之后' },
        });
        fireEvent.click(screen.getByRole('button', { name: '跳过' }));
        expect(actions.question).toHaveBeenLastCalledWith('session-1', 'question-1', {
            cancelled: true,
            values: {},
        });
    });

    it('中文输入法确认文本时不会误发消息，Shift+Enter 保留换行', () => {
        render(
            <Conversation
                session={session()}
                drafts={{ 'session-1': '问题' }}
                pending={[]}
                onNew={vi.fn()}
            />,
        );
        const input = screen.getByRole('textbox', { name: '输入诊断问题' });
        fireEvent.keyDown(input, { key: 'Enter', code: 'Enter', isComposing: true });
        fireEvent.keyDown(input, { key: 'Enter', code: 'Enter', shiftKey: true });
        expect(actions.send).not.toHaveBeenCalled();
        fireEvent.keyDown(input, { key: 'Enter', code: 'Enter' });
        expect(actions.send).toHaveBeenCalledWith('session-1', '问题');
    });
});
