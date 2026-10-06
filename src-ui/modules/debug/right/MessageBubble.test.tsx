import type { ReactNode } from 'react';
import { afterAll, afterEach, beforeAll, describe, expect, it } from 'vitest';
import { act, cleanup, render, screen } from '@testing-library/react';
import { TooltipProvider } from '../../../shared/ui';
import type { MessageItem } from '../../../core/domain/debug/chatFormat';
import { MessageBubble } from './MessageBubble';

// 让「折叠着的正文」的 scrollHeight 可控，并接住 ResizeObserver 的回调，模拟栏宽变了
let contentHeight = 200;
const observers: Array<() => void> = [];
const scrollHeight = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'scrollHeight');
const clientHeight = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'clientHeight');
const RO = window.ResizeObserver;

beforeAll(() => {
    Object.defineProperty(HTMLElement.prototype, 'scrollHeight', {
        configurable: true,
        get() {
            return (this as HTMLElement).style.maxHeight ? contentHeight : 0;
        },
    });
    Object.defineProperty(HTMLElement.prototype, 'clientHeight', {
        configurable: true,
        get() {
            const el = this as HTMLElement;
            return el.style.maxHeight ? Math.min(contentHeight, parseFloat(el.style.maxHeight)) : 0;
        },
    });
    class RecordingRO {
        constructor(private cb: () => void) {}
        observe() {
            observers.push(() => this.cb());
        }
        unobserve() {}
        disconnect() {}
    }
    Object.defineProperty(window, 'ResizeObserver', { writable: true, value: RecordingRO });
    globalThis.ResizeObserver = RecordingRO as unknown as typeof ResizeObserver;
});

afterAll(() => {
    if (scrollHeight) Object.defineProperty(HTMLElement.prototype, 'scrollHeight', scrollHeight);
    if (clientHeight) Object.defineProperty(HTMLElement.prototype, 'clientHeight', clientHeight);
    Object.defineProperty(window, 'ResizeObserver', { writable: true, value: RO });
    globalThis.ResizeObserver = RO;
});

afterEach(() => {
    cleanup();
    observers.length = 0;
    contentHeight = 200;
});

const wrapper = ({ children }: { children: ReactNode }) => (
    <TooltipProvider>{children}</TooltipProvider>
);

function item(text: string): MessageItem {
    return {
        kind: 'message',
        key: 'e1',
        seq: 1,
        at: Date.now(),
        session: 'group:1',
        direction: 'in',
        senderId: 10001,
        senderName: '小明',
        messageId: 1,
        segments: [{ type: 'text', data: { text } }],
        raw: {},
    };
}

describe('MessageBubble', () => {
    it('栏变窄、长消息超过 12 行时才出「展开」', () => {
        render(
            <MessageBubble
                item={item('长'.repeat(200))}
                continued={false}
                selected={false}
                showSessionName={false}
            />,
            {
                wrapper,
            },
        );
        // 宽的时候放得下
        expect(screen.queryByRole('button', { name: '展开' })).not.toBeInTheDocument();

        // 拖窄了：正文变高，ResizeObserver 报尺寸变了
        contentHeight = 400;
        act(() => observers.forEach((fn) => fn()));
        expect(screen.getByRole('button', { name: '展开' })).toBeInTheDocument();
    });

    it('自己发的气泡下面写真正的动作；OB11 失败写 retcode 和上游的说明，retcode 的含义放在悬停提示里', () => {
        const sent: MessageItem = {
            ...item('转一下'),
            direction: 'out',
            senderName: '我',
            call: {
                action: 'send_msg',
                requestId: 'r1',
                ok: true,
                retcode: 0,
                elapsedMs: 88,
                error: null,
                channel: { kind: 'internal' },
            },
        };
        const { rerender } = render(
            <MessageBubble
                item={sent}
                continued={false}
                selected={false}
                showSessionName={false}
            />,
            {
                wrapper,
            },
        );
        expect(screen.getByText('↗ send_msg · ✓ 88ms · 内部通道')).toBeInTheDocument();

        const failed: MessageItem = {
            ...sent,
            call: {
                ...sent.call!,
                action: 'send_group_msg',
                ok: false,
                retcode: 1200,
                wording: '消息内容为空',
            },
        };
        rerender(
            <MessageBubble
                item={failed}
                continued={false}
                selected={false}
                showSessionName={false}
            />,
        );
        const line = screen.getByText('↗ send_group_msg · ✗ retcode 1200 · 消息内容为空');
        expect(line).toHaveAttribute('title', '上游执行出错：具体原因看返回里的 message / wording');
    });

    it('每条消息是一个 article；选中的带 aria-current', () => {
        const { rerender } = render(
            <MessageBubble
                item={item('你好')}
                continued={false}
                selected={false}
                showSessionName={false}
            />,
            { wrapper },
        );
        const row = screen.getByRole('article', { name: /小明：你好/ });
        expect(row).not.toHaveAttribute('aria-current');
        rerender(
            <MessageBubble
                item={item('你好')}
                continued={false}
                selected
                showSessionName={false}
            />,
        );
        expect(screen.getByRole('article', { name: /已选中/ })).toHaveAttribute(
            'aria-current',
            'true',
        );
    });
});
