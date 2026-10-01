import type { ReactNode } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { TooltipProvider } from '../../../shared/ui';
import type { ChatItem } from '../../../core/domain/debug/chat';
import type { MessageItem } from '../../../core/domain/debug/chatFormat';
import { preferencesStore } from '../../../hooks/preferences/preferencesStore';
import { debugWorkspaceStore } from '../../../hooks/debug/debugWorkspaceStore';
import { EventDetailPopover, type DetailTarget } from './EventDetailPopover';

const wrapper = ({ children }: { children: ReactNode }) => <TooltipProvider>{children}</TooltipProvider>;

function message(patch: Partial<MessageItem> = {}): MessageItem {
    return {
        kind: 'message',
        key: 'e1',
        seq: 1,
        at: 1_700_000_000_000,
        session: 'group:100001',
        direction: 'in',
        senderId: 10001,
        senderName: '小明',
        messageId: 4242,
        segments: [{ type: 'text', data: { text: '在吗' } }],
        raw: { post_type: 'message', message_type: 'group', group_id: 100001, user_id: 10001, message_id: 4242 },
        ...patch,
    };
}

function targetOf(item: ChatItem): DetailTarget {
    const anchor = document.createElement('button');
    document.body.appendChild(anchor);
    return { item, anchor };
}

/** 先挂一个关着的弹层再打开：和真实路径一样（对着行点开的），也让进场效果在测试里套用得上 */
function openPopover(item: ChatItem, onClose = vi.fn()) {
    const view = render(<EventDetailPopover target={null} onClose={onClose} />, { wrapper });
    view.rerender(<EventDetailPopover target={targetOf(item)} onClose={onClose} />);
    return { ...view, onClose };
}

beforeEach(() => {
    preferencesStore.setMotionEnabled(false);
    debugWorkspaceStore._reset();
});

afterEach(() => {
    cleanup();
    debugWorkspaceStore._reset();
});

describe('EventDetailPopover 消息快捷操作', () => {
    it('群消息有「回复 / 撤回 / 查发送者」；点「撤回」在中栏开一个预填好的 delete_msg 标签并关掉弹层', async () => {
        const user = userEvent.setup();
        const open = vi.spyOn(debugWorkspaceStore, 'openAction');
        const { onClose } = openPopover(message());

        await user.click(await screen.findByRole('button', { name: '撤回' }));
        expect(open).toHaveBeenCalledWith('delete_msg', { newTab: true, paramsText: '{\n  "message_id": 4242\n}' });
        expect(onClose).toHaveBeenCalledTimes(1);
    });

    it('群消息的「回复」开 send_group_msg（带 reply 段和群号）', async () => {
        const user = userEvent.setup();
        const open = vi.spyOn(debugWorkspaceStore, 'openAction');
        openPopover(message());

        await user.click(await screen.findByRole('button', { name: '回复' }));
        expect(open).toHaveBeenCalledWith('send_group_msg', {
            newTab: true,
            paramsText: '{\n  "group_id": 100001,\n  "message": [\n    {\n      "type": "reply",\n      "data": {\n        "id": 4242\n      }\n    }\n  ]\n}',
        });
    });

    it('私聊消息的「查发送者」开 get_stranger_info；自己发的气泡没有「查发送者」', async () => {
        const user = userEvent.setup();
        const open = vi.spyOn(debugWorkspaceStore, 'openAction');
        const { rerender, onClose } = openPopover(message({ session: 'private:10001' }));
        await user.click(await screen.findByRole('button', { name: '查发送者' }));
        expect(open).toHaveBeenCalledWith('get_stranger_info', { newTab: true, paramsText: '{\n  "user_id": 10001\n}' });

        rerender(<EventDetailPopover target={targetOf(message({ direction: 'out', senderName: '我' }))} onClose={onClose} />);
        expect(screen.queryByRole('button', { name: '查发送者' })).not.toBeInTheDocument();
        // 回复和撤回还在
        expect(await screen.findByRole('button', { name: '回复' })).toBeInTheDocument();
    });

    it('发送失败的气泡（没有 message_id）没有「回复 / 撤回」；通知没有操作行', async () => {
        const failed = message();
        delete failed.messageId;
        const { rerender, onClose } = openPopover(failed);
        expect(screen.queryByRole('button', { name: '回复' })).not.toBeInTheDocument();
        expect(screen.queryByRole('button', { name: '撤回' })).not.toBeInTheDocument();

        const notice: ChatItem = { kind: 'notice', key: 'e2', seq: 2, at: 1, text: '小明 加入了群', raw: {} };
        rerender(<EventDetailPopover target={targetOf(notice)} onClose={onClose} />);
        expect(screen.queryByRole('button', { name: '回复' })).not.toBeInTheDocument();
        expect(screen.queryByRole('button', { name: '查发送者' })).not.toBeInTheDocument();
    });
});
