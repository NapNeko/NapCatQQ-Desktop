import { act, renderHook } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { Virtualizer } from '@tanstack/react-virtual';
import type { WheelEvent } from 'react';
import { useStickToBottom } from './useStickToBottom';

const CLIENT_HEIGHT = 300;

interface Item {
    key: string;
}

const originalResizeObserver = globalThis.ResizeObserver;

afterEach(() => {
    globalThis.ResizeObserver = originalResizeObserver;
    vi.unstubAllGlobals();
});

// 只给元素铺一层滚动几何：jsdom 没有布局，scrollHeight / clientHeight 全靠 getter 造。
// scrollToIndex 模拟虚拟器「滚到底」的效果——把 scrollTop 钉到 scrollHeight - clientHeight。
function setup(opts: { followUntilUserScroll?: boolean; reattachOnIntent?: boolean } = {}) {
    const element = document.createElement('div');
    let height = 2400;
    Object.defineProperties(element, {
        clientHeight: { configurable: true, value: CLIENT_HEIGHT },
        scrollHeight: { configurable: true, get: () => height },
    });
    let items: Item[] = [{ key: 'm1' }];
    const voptions = { count: items.length };
    const virtualizer = {
        options: voptions,
        scrollToIndex: vi.fn(() => {
            element.scrollTop = Math.max(0, height - CLIENT_HEIGHT);
        }),
    } as unknown as Virtualizer<HTMLDivElement, Element>;

    const hook = renderHook(
        ({ items: current }: { items: Item[] }) =>
            useStickToBottom<Item>({
                scrollRef: { current: element },
                virtualizer,
                items: current,
                memoryKey: null,
                resetToken: 'test',
                filterToken: '',
                animate: false,
                reattachOnIntent: opts.reattachOnIntent,
                followUntilUserScroll: opts.followUntilUserScroll,
            }),
        { initialProps: { items } },
    );

    return {
        element,
        virtualizer,
        hook,
        wheel: (deltaY: number) =>
            act(() => hook.result.current.handlers.onWheel({ deltaY } as WheelEvent<HTMLElement>)),
        scroll: () => act(() => hook.result.current.handlers.onScroll()),
        setHeight: (value: number) => {
            height = value;
        },
        // 追加 n 条并触发一次提交；grownTo 模拟内容撑高后的 scrollHeight
        push: (n: number, grownTo?: number) => {
            if (grownTo !== undefined) height = grownTo;
            const base = items.length;
            items = [
                ...items,
                ...Array.from({ length: n }, (_, i) => ({ key: `m${base + i + 1}` })),
            ];
            voptions.count = items.length;
            hook.rerender({ items });
        },
    };
}

describe('useStickToBottom（虚拟列表模式）', () => {
    it('初始挂载就贴底，内容增高后自动补滚到底', () => {
        const { element, virtualizer, hook, push } = setup();
        expect(virtualizer.scrollToIndex).toHaveBeenCalledWith(0, { align: 'end' });
        expect(hook.result.current.isFollowing()).toBe(true);
        expect(hook.result.current.unseen).toBe(0);

        // 新条目挂上、容器撑高：贴底状态跟着滚到新的底部
        push(1, 2700);
        expect(virtualizer.scrollToIndex).toHaveBeenCalledWith(1, { align: 'end' });
        expect(element.scrollTop).toBe(2400);
        expect(hook.result.current.unseen).toBe(0);
        expect(hook.result.current.isFollowing()).toBe(true);
    });

    it('离底不到 120px 还算贴底，超过阈值才放开；放开后新内容只计数不拉回', () => {
        const { element, virtualizer, hook, scroll, push } = setup();
        const callsAtMount = vi.mocked(virtualizer.scrollToIndex).mock.calls.length;

        // 滚轮没动，只是滚动事件把位置带离底部：100px 以内不算翻上去
        element.scrollTop = 2000; // 离底 100
        scroll();
        expect(hook.result.current.isFollowing()).toBe(true);

        element.scrollTop = 1900; // 离底 200，越过 STICK_PX
        scroll();
        expect(hook.result.current.isFollowing()).toBe(false);

        push(1, 2700);
        expect(hook.result.current.unseen).toBe(1);
        expect(element.scrollTop).toBe(1900);
        expect(vi.mocked(virtualizer.scrollToIndex).mock.calls.length).toBe(callsAtMount);
    });

    it('滚轮向上立即放开贴底且不拉回；向下滚回 120px 以内重新贴底并继续跟随', () => {
        const { element, virtualizer, hook, wheel, scroll, push } = setup();
        const callsAtMount = vi.mocked(virtualizer.scrollToIndex).mock.calls.length;

        // 刷屏时离底还在 120px 内也要放开，否则翻不上去
        wheel(-120);
        element.scrollTop = 1000;
        scroll();
        expect(hook.result.current.isFollowing()).toBe(false);
        expect(hook.result.current.away).toBe(true);

        push(2);
        expect(hook.result.current.unseen).toBe(2);
        expect(element.scrollTop).toBe(1000);
        expect(vi.mocked(virtualizer.scrollToIndex).mock.calls.length).toBe(callsAtMount);

        // 用户自己往下滚，回到离底 50px：重新贴底，计数清零
        wheel(120);
        element.scrollTop = 2050; // 离底 50，且比上一次滚动更靠下
        scroll();
        expect(hook.result.current.isFollowing()).toBe(true);
        expect(hook.result.current.unseen).toBe(0);

        push(1, 2700);
        expect(virtualizer.scrollToIndex).toHaveBeenLastCalledWith(3, { align: 'end' });
        expect(element.scrollTop).toBe(2400);
        expect(hook.result.current.unseen).toBe(0);
    });
});

describe('useStickToBottom（原生滚动 followUntilUserScroll 模式）', () => {
    // 这一模式贴底是直接写 scrollTop，内容撑高靠 ResizeObserver 的下一帧补滚。
    // jsdom 没有 ResizeObserver，帧回调也拿不到，都换成手动触发的桩。
    function setupNative() {
        let frame: FrameRequestCallback | undefined;
        let resized: ResizeObserverCallback | undefined;
        // RO 在挂载的 layout effect 里就 new，桩必须先于 renderHook 装上
        globalThis.ResizeObserver = class {
            constructor(callback: ResizeObserverCallback) {
                resized = callback;
            }
            observe() {}
            unobserve() {}
            disconnect() {
                resized = undefined;
            }
        };
        vi.stubGlobal(
            'requestAnimationFrame',
            vi.fn((callback: FrameRequestCallback) => {
                frame = callback;
                return 1;
            }),
        );
        vi.stubGlobal(
            'cancelAnimationFrame',
            vi.fn(() => {
                frame = undefined;
            }),
        );
        const base = setup({ followUntilUserScroll: true, reattachOnIntent: true });
        // 内容撑高：RO 回调里先 cancel 再约下一帧，这里把两步串起来手动跑
        const contentGrew = () => {
            act(() => {
                resized?.([], {} as ResizeObserver);
                const run = frame;
                frame = undefined;
                run?.(0);
            });
        };
        return { ...base, contentGrew };
    }

    it('撑高后下一帧补回底部；上翻打断后不拉回，滚回底部恢复跟随', () => {
        const { element, virtualizer, hook, wheel, scroll, setHeight, contentGrew } = setupNative();
        // 挂载即钉到底（2400-300），且走的是直接写 scrollTop、不经过虚拟器
        expect(element.scrollTop).toBe(2100);
        expect(virtualizer.scrollToIndex).not.toHaveBeenCalled();
        expect(hook.result.current.isFollowing()).toBe(true);

        setHeight(2600);
        contentGrew();
        expect(element.scrollTop).toBe(2300);

        // 用户往上翻：之后的内容撑高不再拉回
        wheel(-120);
        setHeight(2900);
        contentGrew();
        expect(element.scrollTop).toBe(2300);
        expect(hook.result.current.isFollowing()).toBe(false);

        // 带向下意图滚回最底：重新贴底，之后的撑高继续跟着走
        wheel(120);
        element.scrollTop = 2600;
        scroll();
        expect(hook.result.current.isFollowing()).toBe(true);

        setHeight(3100);
        contentGrew();
        expect(element.scrollTop).toBe(2800);
    });
});
