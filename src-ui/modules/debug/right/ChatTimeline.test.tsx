import type { ReactNode } from 'react';
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it } from 'vitest';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { TooltipProvider } from '../../../shared/ui';
import { preferencesStore } from '../../../core/domain/settings/preferencesStore';
import { _resetDebugScrollMemoryForTests } from '../../../hooks/debug/debugScrollMemory';
import type { ChatItem } from '../../../core/domain/debug/chat';
import { ChatTimeline, type ChatTimelineProps } from './ChatTimeline';

// jsdom 里元素都没有尺寸：滚动容器给 400px 高，每行 50px，虚拟列表才会画出行来
const offsetHeight = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'offsetHeight');
const offsetWidth = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'offsetWidth');
const VIEW = 400;

beforeAll(() => {
    Object.defineProperty(HTMLElement.prototype, 'offsetHeight', {
        configurable: true,
        get() {
            return (this as HTMLElement).dataset.testid === 'chat-scroller' ? VIEW : 50;
        },
    });
    Object.defineProperty(HTMLElement.prototype, 'offsetWidth', {
        configurable: true,
        get: () => 380,
    });
});

afterAll(() => {
    if (offsetHeight) Object.defineProperty(HTMLElement.prototype, 'offsetHeight', offsetHeight);
    if (offsetWidth) Object.defineProperty(HTMLElement.prototype, 'offsetWidth', offsetWidth);
});

beforeEach(() => {
    preferencesStore.setMotionEnabled(false);
});

afterEach(() => {
    cleanup();
    _resetDebugScrollMemoryForTests();
    preferencesStore.reset();
});

const BASE = new Date(2026, 8, 29, 14, 0, 0).getTime();

function message(seq: number, text = `第 ${seq} 条`): ChatItem {
    return {
        kind: 'message',
        key: `e${seq}`,
        seq,
        at: BASE + seq * 1000,
        session: 'group:100001',
        direction: 'in',
        senderId: 10001,
        senderName: '小明',
        messageId: 5000 + seq,
        segments: [{ type: 'text', data: { text } }],
        raw: {},
    };
}

function messages(from: number, to: number): ChatItem[] {
    const out: ChatItem[] = [];
    for (let s = from; s <= to; s += 1) out.push(message(s));
    return out;
}

const wrapper = ({ children }: { children: ReactNode }) => (
    <TooltipProvider>{children}</TooltipProvider>
);

function props(items: ChatItem[], patch: Partial<ChatTimelineProps> = {}): ChatTimelineProps {
    return {
        botId: 'bot-1',
        items,
        selectedKey: null,
        showSessionName: false,
        trimmed: 0,
        resetToken: 'all|0|0',
        filterToken: 'all',
        paused: false,
        ...patch,
    };
}

/**
 * 给滚动容器装上可控的滚动几何：内容高度跟着虚拟列表撑出来的总高走（和浏览器里一致），
 * scrollTop 由测试设；记下程序调了哪些 scrollTo（目标位置），用来判断有没有被「拽回底部」。
 * 和浏览器一样，位置真的变了才发 scroll 事件：滚不动的时候没有事件能把贴底接回来
 */
function scrollMetrics(el: HTMLElement) {
    let top = 0;
    // 每次程序滚动的目标，连同当时的最底位置（内容高度会随着行被量出来而变）
    const calls: Array<{ to: number; max: number }> = [];
    const height = () =>
        parseFloat((el.firstElementChild as HTMLElement | null)?.style.height ?? '') || VIEW;
    const maxTop = () => Math.max(0, height() - VIEW);
    const move = (to: number) => {
        const next = Math.max(0, Math.min(to, maxTop()));
        if (next === top) return;
        top = next;
        fireEvent.scroll(el);
    };
    Object.defineProperty(el, 'scrollHeight', { configurable: true, get: height });
    Object.defineProperty(el, 'clientHeight', { configurable: true, get: () => VIEW });
    Object.defineProperty(el, 'scrollTop', {
        configurable: true,
        get: () => top,
        set: (v: number) => {
            top = Math.max(0, Math.min(v, maxTop()));
        },
    });
    (el as HTMLElement & { scrollTo: (o: ScrollToOptions) => void }).scrollTo = (
        o: ScrollToOptions,
    ) => {
        if (typeof o.top !== 'number') return;
        calls.push({ to: o.top, max: maxTop() });
        move(o.top);
    };
    return {
        calls,
        get top() {
            return top;
        },
        get maxTop() {
            return maxTop();
        },
        /** 程序有没有把视口滚到最底（被拽回去了） */
        pulledToBottom(since: number) {
            return calls.slice(since).some((c) => c.to >= c.max - 1);
        },
        scrollTo(value: number) {
            move(value);
        },
        scrollToBottom() {
            this.scrollTo(maxTop());
        },
    };
}

type Played = { el: Element; frames: Keyframe[] };

/** jsdom 没有 WAAPI：换成记录每次 animate 的元素和关键帧 */
function recordAnimations(): { played: Played[]; restore: () => void } {
    const played: Played[] = [];
    const original = HTMLElement.prototype.animate;
    HTMLElement.prototype.animate = function (
        this: HTMLElement,
        frames: Keyframe[] | PropertyIndexedKeyframes | null,
    ) {
        played.push({ el: this, frames: (frames ?? []) as Keyframe[] });
        return { cancel() {} } as unknown as Animation;
    };
    return {
        played,
        restore: () => {
            HTMLElement.prototype.animate = original;
        },
    };
}

const rowText = (p: Played) => p.el.closest('[data-index]')?.textContent ?? '';

describe('ChatTimeline', () => {
    it('贴着底时来了新消息不出「新消息」胶囊；往上翻了之后再来，显示来了几条，点了回到底部', () => {
        const { rerender } = render(<ChatTimeline {...props(messages(1, 20))} />, { wrapper });
        const m = scrollMetrics(screen.getByTestId('chat-scroller'));

        // 在底部（离底 0px）
        m.scrollToBottom();
        rerender(<ChatTimeline {...props(messages(1, 22))} />);
        expect(screen.queryByText(/条新消息/)).not.toBeInTheDocument();

        // 离底不到 120px 也算贴底（没有表现出往上翻的意思）
        m.scrollTo(m.maxTop - 100);
        rerender(<ChatTimeline {...props(messages(1, 23))} />);
        expect(screen.queryByText(/条新消息/)).not.toBeInTheDocument();

        // 翻到顶上：新来 3 条
        m.scrollTo(0);
        rerender(<ChatTimeline {...props(messages(1, 26))} />);
        expect(screen.getByRole('button', { name: '有 3 条新消息，回到底部' })).toHaveTextContent(
            '3 条新消息',
        );

        // 又来 2 条，累加
        rerender(<ChatTimeline {...props(messages(1, 28))} />);
        expect(screen.getByText('5 条新消息')).toBeInTheDocument();

        const before = m.calls.length;
        fireEvent.click(screen.getByRole('button', { name: '有 5 条新消息，回到底部' }));
        expect(screen.queryByText(/条新消息/)).not.toBeInTheDocument();
        expect(m.pulledToBottom(before)).toBe(true);
    });

    it('自己滚回底部时胶囊消失', () => {
        const { rerender } = render(<ChatTimeline {...props(messages(1, 20))} />, { wrapper });
        const m = scrollMetrics(screen.getByTestId('chat-scroller'));
        m.scrollTo(100);
        rerender(<ChatTimeline {...props(messages(1, 21))} />);
        expect(screen.getByText('1 条新消息')).toBeInTheDocument();

        m.scrollTo(m.maxTop - 10);
        expect(screen.queryByText(/条新消息/)).not.toBeInTheDocument();
    });

    it('刷屏时往上拨一小格滚轮就放开贴底：不会被一帧帧的新消息拽回去；自己滚回底部附近才重新跟随', () => {
        const { rerender } = render(<ChatTimeline {...props(messages(1, 20))} />, { wrapper });
        const scroller = screen.getByTestId('chat-scroller');
        const m = scrollMetrics(scroller);
        m.scrollToBottom();

        const since = m.calls.length;
        let held = 0;
        // 每一步：滚轮往上一小格（仍在离底 120px 以内），紧接着又来一条
        for (let k = 1; k <= 5; k += 1) {
            fireEvent.wheel(scroller, { deltaY: -40 });
            held = m.top - 20;
            m.scrollTo(held);
            rerender(<ChatTimeline {...props(messages(1, 20 + k))} />);
        }
        // 一次也没被滚回底部，停在用户拨到的位置
        expect(m.pulledToBottom(since)).toBe(false);
        expect(m.top).toBe(held);
        expect(screen.getByText('5 条新消息')).toBeInTheDocument();

        // 往下滚回 120px 以内：重新贴底，胶囊消失，再来新的就跟着到底
        fireEvent.wheel(scroller, { deltaY: 40 });
        m.scrollTo(m.maxTop - 30);
        expect(screen.queryByText(/条新消息/)).not.toBeInTheDocument();
        const again = m.calls.length;
        rerender(<ChatTimeline {...props(messages(1, 26))} />);
        expect(m.pulledToBottom(again)).toBe(true);
    });

    it('PageUp 也算往上翻：还在 120px 以内也不再跟随', () => {
        const { rerender } = render(<ChatTimeline {...props(messages(1, 20))} />, { wrapper });
        const scroller = screen.getByTestId('chat-scroller');
        const m = scrollMetrics(scroller);
        m.scrollToBottom();
        fireEvent.keyDown(scroller, { key: 'PageUp' });
        m.scrollTo(m.maxTop - 10);
        const since = m.calls.length;
        rerender(<ChatTimeline {...props(messages(1, 21))} />);
        expect(m.pulledToBottom(since)).toBe(false);
        expect(screen.getByText('1 条新消息')).toBeInTheDocument();
    });

    it('内容还不满一屏、或已经在最顶上时往上拨滚轮 / 按 PageUp：没有地方可翻，不放开贴底，不冒假的「新消息」', () => {
        const { rerender } = render(<ChatTimeline {...props(messages(1, 3))} />, { wrapper });
        const scroller = screen.getByTestId('chat-scroller');
        const m = scrollMetrics(scroller);
        expect(m.maxTop).toBe(0);

        fireEvent.wheel(scroller, { deltaY: -120 });
        fireEvent.keyDown(scroller, { key: 'PageUp' });
        fireEvent.pointerDown(scroller);
        rerender(<ChatTimeline {...props(messages(1, 4))} />);
        expect(screen.queryByText(/条新消息/)).not.toBeInTheDocument();

        // 长到超过一屏之后照样贴着底跟随
        const since = m.calls.length;
        rerender(<ChatTimeline {...props(messages(1, 30))} />);
        expect(screen.queryByText(/条新消息/)).not.toBeInTheDocument();
        expect(m.pulledToBottom(since)).toBe(true);
    });

    it('换了会话（resetToken 变了）不算新消息', () => {
        const { rerender } = render(<ChatTimeline {...props(messages(1, 20))} />, { wrapper });
        const m = scrollMetrics(screen.getByTestId('chat-scroller'));
        m.scrollTo(0);
        rerender(<ChatTimeline {...props(messages(30, 40), { resetToken: 'group:1|0|0' })} />);
        expect(screen.queryByText(/条新消息/)).not.toBeInTheDocument();
    });

    it('进场动画只播一次：这一批的行滚走再滚回来不重播', () => {
        const rec = recordAnimations();
        preferencesStore.setMotionEnabled(true);
        try {
            const { rerender } = render(<ChatTimeline {...props(messages(1, 30))} />, { wrapper });
            const m = scrollMetrics(screen.getByTestId('chat-scroller'));
            m.scrollToBottom();
            rerender(<ChatTimeline {...props(messages(1, 31))} />);
            const enters = () =>
                rec.played.filter(
                    (p) => 'transform' in (p.frames[0] ?? {}) && rowText(p).includes('第 31 条'),
                );
            expect(enters()).toHaveLength(1);

            // 滚到顶上（第 31 条的行被虚拟列表卸掉），再滚回来（重新挂上）；直接改位置，不带滚轮意图
            m.scrollTo(0);
            expect(screen.queryByText('第 31 条')).not.toBeInTheDocument();
            m.scrollToBottom();
            expect(screen.getByText('第 31 条')).toBeInTheDocument();
            expect(enters()).toHaveLength(1);
        } finally {
            rec.restore();
        }
    });

    it('跳到被回复的消息闪一下；闪完之后那一行被卸了再挂上不重闪', async () => {
        const rec = recordAnimations();
        try {
            let reveal: ((id: number) => boolean) | null = null;
            render(
                <ChatTimeline
                    {...props(messages(1, 30))}
                    onRevealReady={(fn) => {
                        reveal = fn;
                    }}
                />,
                { wrapper },
            );
            const m = scrollMetrics(screen.getByTestId('chat-scroller'));
            m.scrollToBottom();
            const flashes = () =>
                rec.played.filter(
                    (p) =>
                        (p.frames[0] as { opacity?: number } | undefined)?.opacity === 0.85 &&
                        rowText(p).includes('第 2 条'),
                );

            act(() => {
                expect(reveal?.(5002)).toBe(true);
            });
            expect(screen.getByText('第 2 条')).toBeInTheDocument();
            expect(flashes()).toHaveLength(1);

            await act(async () => {
                await new Promise((r) => setTimeout(r, 1600));
            });
            m.scrollToBottom();
            expect(screen.queryByText('第 2 条')).not.toBeInTheDocument();
            m.scrollTo(0);
            expect(screen.getByText('第 2 条')).toBeInTheDocument();
            expect(flashes()).toHaveLength(1);
        } finally {
            rec.restore();
        }
    });

    it('断线缺口、上游丢弃、缓冲裁掉都有提示行', () => {
        const items: ChatItem[] = [
            message(1, '断线前'),
            {
                kind: 'gap',
                key: 'e2',
                seq: 2,
                at: BASE + 2000,
                fromMs: BASE + 1000,
                toMs: BASE + 65_000,
            },
            { kind: 'dropped', key: 'e3', seq: 3, at: BASE + 3000, count: 42 },
            message(4, '恢复后'),
        ];
        render(<ChatTimeline {...props(items, { trimmed: 1234 })} />, { wrapper });
        expect(screen.getByText('14:00:01–14:01:05 断开期间可能漏了事件')).toBeInTheDocument();
        expect(screen.getByText('上游丢了 42 条（接收太慢）')).toBeInTheDocument();
        expect(screen.getByText('更早的 1,234 条已丢弃')).toBeInTheDocument();
        expect(screen.getByText('断线前')).toBeInTheDocument();
        expect(screen.getByText('恢复后')).toBeInTheDocument();
    });

    it('隔了 5 分钟以上、或跨过零点插时间分隔线；同一人紧接着发的不重复画名字', () => {
        const a = message(1, '第一句');
        const b = { ...message(2, '第二句'), at: BASE + 30_000 } as ChatItem;
        const c = { ...message(3, '隔了很久'), at: BASE + 10 * 60_000 } as ChatItem;
        const midnight = new Date(2026, 8, 30, 0, 0, 0).getTime();
        const d = { ...message(4, '零点前'), at: midnight - 60_000 } as ChatItem;
        const e = { ...message(5, '零点后'), at: midnight + 60_000 } as ChatItem;
        render(<ChatTimeline {...props([a, b, c, d, e])} />, { wrapper });
        const separators = screen.getAllByRole('separator');
        // a、c、d（隔了很久）、e（只隔 2 分钟但跨了天）
        expect(separators).toHaveLength(4);
        expect(separators[0]).toHaveTextContent(/14:00$/);
        expect(separators[1]).toHaveTextContent(/14:10$/);
        expect(separators[3]).toHaveTextContent(/00:01$/);
        // 第二句是紧接着的：它没有名字
        expect(screen.getAllByText('小明')).toHaveLength(4);
    });
});
