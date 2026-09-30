import { useRef } from 'react';
import { afterEach, describe, expect, it } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import {
    _resetDebugScrollMemoryForTests,
    forgetScroll,
    recallScroll,
    rememberScroll,
    useScrollMemory,
} from './debugScrollMemory';

function LateScroller({ ready, variant = 'a' }: { ready: boolean; variant?: 'a' | 'b' }) {
    const ref = useRef<HTMLDivElement>(null);
    useScrollMemory('history', ref);
    if (!ready) return <p>还没有历史</p>;
    // variant 换了就是另一个容器节点（key 不同，React 会卸掉旧的、挂新的）
    return <div key={variant} ref={ref} data-testid={`list-${variant}`} />;
}

function Scroller({ memoryKey, stuck = false }: { memoryKey: string | null; stuck?: boolean }) {
    const ref = useRef<HTMLDivElement>(null);
    useScrollMemory(memoryKey, ref, { shouldRemember: () => !stuck });
    return <div ref={ref} data-testid="box" />;
}

afterEach(() => {
    cleanup();
    _resetDebugScrollMemoryForTests();
});

describe('rememberScroll / recallScroll', () => {
    it('记、取、忘', () => {
        rememberScroll('catalog', 120);
        expect(recallScroll('catalog')).toBe(120);
        rememberScroll('catalog', null);
        expect(recallScroll('catalog')).toBeUndefined();
        rememberScroll('history', -5);
        expect(recallScroll('history')).toBe(0);
        forgetScroll('history');
        expect(recallScroll('history')).toBeUndefined();
    });
});

describe('useScrollMemory', () => {
    it('挂上时恢复，滚动时记下；卸载再挂回来还在原处', () => {
        rememberScroll('catalog', 80);
        const { unmount } = render(<Scroller memoryKey="catalog" />);
        const box = screen.getByTestId('box');
        expect(box.scrollTop).toBe(80);

        box.scrollTop = 200;
        fireEvent.scroll(box);
        expect(recallScroll('catalog')).toBe(200);

        unmount();
        render(<Scroller memoryKey="catalog" />);
        expect(screen.getByTestId('box').scrollTop).toBe(200);
    });

    it('shouldRemember 返回 false 时忘掉（聊天贴着底）', () => {
        rememberScroll('chat:a', 50);
        render(<Scroller memoryKey="chat:a" stuck />);
        const box = screen.getByTestId('box');
        fireEvent.scroll(box);
        expect(recallScroll('chat:a')).toBeUndefined();
    });

    it('键为 null 时什么也不做', () => {
        render(<Scroller memoryKey={null} />);
        const box = screen.getByTestId('box');
        box.scrollTop = 30;
        fireEvent.scroll(box);
        expect(recallScroll('null')).toBeUndefined();
    });

    it('滚动容器晚于调用方出现（先是空状态）也能接上；换了容器会撤旧接新', () => {
        rememberScroll('history', 140);
        const { rerender } = render(<LateScroller ready={false} />);
        expect(recallScroll('history')).toBe(140);

        rerender(<LateScroller ready />);
        const first = screen.getByTestId('list-a');
        expect(first.scrollTop).toBe(140);
        first.scrollTop = 60;
        fireEvent.scroll(first);
        expect(recallScroll('history')).toBe(60);

        rerender(<LateScroller ready variant="b" />);
        const second = screen.getByTestId('list-b');
        expect(second.scrollTop).toBe(60);
        // 旧节点上的监听已经撤掉：它再滚也不该改记忆
        first.scrollTop = 999;
        fireEvent.scroll(first);
        expect(recallScroll('history')).toBe(60);
        second.scrollTop = 75;
        fireEvent.scroll(second);
        expect(recallScroll('history')).toBe(75);
    });
});
