// 调试台各块滚动位置的记忆：切到别的路由再回来、收起再展开某一栏，列表还停在原处。
//
// 放在模块里而不是组件里：页面卸载时组件里的一切都没了（frontend.md 坑 6）。只记在内存里，
// 重启程序不恢复——那时列表内容多半已经变了，停在老位置反而找不着。
//
// 键约定：`catalog`、`history`、`chat:<botId>`、`response:<tabId>`。
// 聊天只在「没贴着底」时记：贴底的时间线回来时应该还是贴底看最新的，调用方用 shouldRemember 表达这一点。

import { useLayoutEffect, useRef, type RefObject } from 'react';

const memory = new Map<string, number>();

/** 记下某块的滚动位置；传 null 等于忘掉 */
export function rememberScroll(key: string, top: number | null): void {
    if (top === null || !Number.isFinite(top)) memory.delete(key);
    else memory.set(key, Math.max(0, top));
}

/** 取回记下的位置；没记过是 undefined */
export function recallScroll(key: string): number | undefined {
    return memory.get(key);
}

export function forgetScroll(key: string): void {
    memory.delete(key);
}

interface ScrollMemoryOptions {
    /**
     * 滚动时问一下这个位置值不值得记；返回 false 就忘掉这个键。
     * 聊天时间线用它：贴着底时返回 false，回来后由时间线自己滚到底。
     */
    shouldRemember?: (el: HTMLElement) => boolean;
}

/** 挂上一个容器：先恢复位置，再开始记。返回撤掉的函数 */
function attach(
    el: HTMLElement,
    key: string,
    shouldRemember: () => ScrollMemoryOptions['shouldRemember'],
): () => void {
    const saved = memory.get(key);
    let frame = 0;
    if (saved !== undefined) {
        el.scrollTop = saved;
        if (Math.abs(el.scrollTop - saved) > 1) {
            frame = requestAnimationFrame(() => {
                frame = 0;
                el.scrollTop = saved;
            });
        }
    }

    // 每次滚动只是往 Map 里写一个数，不值得再节流
    const onScroll = () => {
        const check = shouldRemember();
        if (check && !check(el)) memory.delete(key);
        else memory.set(key, el.scrollTop);
    };
    el.addEventListener('scroll', onScroll, { passive: true });
    return () => {
        if (frame) cancelAnimationFrame(frame);
        el.removeEventListener('scroll', onScroll);
    };
}

/**
 * 把一个滚动容器接到记忆上：挂上（或换键、换了容器）时恢复，滚动时记下。
 *
 * 滚动容器常常晚于调用方出现（先是空状态、数据来了才画列表），也可能被整个换掉，
 * 所以每次渲染后都看一眼 ref 指向的是不是还是接着的那个，不是就撤旧的、接新的。
 * 容器得由调用这个 hook 的组件自己画（它换了，这个组件一定重渲过）。
 *
 * 虚拟列表首帧的总高可能还没撑开，scrollTop 会被夹小；所以下一帧再对一次，
 * 那时 virtualizer 已经按估算高度撑好了。键为 null 时什么也不做（比如还没选中标签）。
 */
export function useScrollMemory(
    key: string | null,
    ref: RefObject<HTMLElement | null>,
    options: ScrollMemoryOptions = {},
): void {
    // 只在滚动时读最新的判断；调用方传内联函数时不该让它反复重挂、反复恢复位置
    const shouldRememberRef = useRef(options.shouldRemember);
    shouldRememberRef.current = options.shouldRemember;
    const attached = useRef<{ el: HTMLElement; key: string; detach: () => void } | null>(null);

    // 故意不给依赖：比一下就走，代价只是一次引用比较
    useLayoutEffect(() => {
        const el = ref.current;
        const cur = attached.current;
        if (cur && cur.el === el && cur.key === key) return;
        cur?.detach();
        attached.current = null;
        if (!el || key === null) return;
        attached.current = { el, key, detach: attach(el, key, () => shouldRememberRef.current) };
    });

    useLayoutEffect(
        () => () => {
            attached.current?.detach();
            attached.current = null;
        },
        [],
    );
}

/** 测试用 */
export function _resetDebugScrollMemoryForTests(): void {
    memory.clear();
}
