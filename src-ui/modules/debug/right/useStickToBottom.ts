// 虚拟列表的「贴底」：离底部不到 120px 算在看最新的，新条目进来跟着滚到底；
// 往上翻了就不拽回来，改成数一数翻上去之后又来了几条，给「↓ N 条新消息」用。
//
// 聊天视图和列表视图共用。滚动位置记在 debugScrollMemory 里：贴着底时不记，回来时还是贴底看最新的。
//
// 几处容易出错的地方：
// - 刷屏时每帧都在追加：光看「离底不到 120px」的话，用户滚轮往上拨一小格还在 120px 以内，
//   下一帧又被拽回底部，等于翻不上去。所以用户一表现出往上翻的意思（滚轮向上、PageUp / ↑ / Home、
//   手指往下拖、按住滚动条）就立刻放开贴底，直到他自己滚回底部（到最底，或者往下滚进 120px 以内）、或者点了胶囊。
// - 往上翻的意思只在真能往上滚时才算：内容还不满一屏、或者已经在最顶上时，滚轮 / PageUp 不会带来任何滚动事件，
//   放开了就再也等不到「滚回底部」把它接回来，下一条新消息会冒出一个假的「N 条新消息」。
// - 平滑滚到底的途中，每一帧离底部都还远，不能据此判成「用户往上翻了」——途中只认「到底了」，
//   用户自己动了滚轮 / 按键 / 拖滚动条才算打断。
// - 新条目的行在挂上那一刻要知道自己是不是「刚来的」（进场动画），而这一刻 effect 还没跑，
//   所以「从第几条开始是新的」在渲染时按上一次提交的末尾算出来（纯读 ref，不改）。
//   这一批里当时没挂上的行记进 pendingEnter，挂上时补播一次；已经播过的不再记，滚走再滚回来不重播。
// - 缓冲满了从头丢条目时，列表整体上移；virtualizer 用 anchorTo: 'end' 把视口顶上那条钉住，这里不用管。

import {
    useCallback,
    useEffect,
    useLayoutEffect,
    useMemo,
    useRef,
    useState,
    type KeyboardEvent,
    type PointerEvent,
    type RefObject,
    type TouchEvent,
    type WheelEvent,
} from 'react';
import type { Virtualizer } from '@tanstack/react-virtual';
import {
    forgetScroll,
    recallScroll,
    useScrollMemory,
} from '../../../hooks/debug/debugScrollMemory';

const STICK_PX = 120;
/** 一批来得太多（刷屏、积压补发）就不做进场动画，满屏一起动反而看不清 */
const ENTER_MAX_BATCH = 8;
/** 平滑滚到底最多等这么久 */
const AUTO_SCROLL_GRACE_MS = 900;
/** 用户放开贴底之后，滚到离底这么近才算「回到最底了」 */
const REATTACH_PX = 4;
const UP_KEYS: ReadonlySet<string> = new Set(['PageUp', 'ArrowUp', 'Home']);
const DOWN_KEYS: ReadonlySet<string> = new Set(['PageDown', 'ArrowDown', 'End']);

function distanceFromBottom(el: HTMLElement): number {
    return el.scrollHeight - el.scrollTop - el.clientHeight;
}

function isStuck(el: HTMLElement): boolean {
    return distanceFromBottom(el) < STICK_PX;
}

const shouldRemember = (el: HTMLElement) => !isStuck(el);

interface Keyed {
    key: string;
}

/** prevLast 之后追加了几条；prevLast 不在列表里（换了会话、被筛掉）返回 -1 */
function appendedAfter(items: readonly Keyed[], prevLast: string | null): number {
    if (prevLast === null) return items.length;
    // 通常就在末尾附近，从后往前找
    for (let i = items.length - 1; i >= 0; i -= 1) {
        if (items[i]?.key === prevLast) return items.length - 1 - i;
    }
    return -1;
}

export interface StickToBottom {
    /** 翻上去之后新来的条数 */
    unseen: number;
    /** 离底部超过一屏：没有新条目时也给一个「回到最新」 */
    away: boolean;
    /** 下标不小于它的行是这次提交刚追加、且此刻贴着底的（做进场动画）；没有是 Infinity */
    enterFrom: number;
    /** 行挂上时问一下：是不是之前某批里还没来得及挂上的新条目（问过就算播过，不会再给第二次） */
    takeEnter: (key: string) => boolean;
    /** 挂到滚动容器上的事件：滚动、滚轮、按键、触摸、按下（滚动条） */
    handlers: {
        onScroll: () => void;
        onWheel: (e: WheelEvent<HTMLElement>) => void;
        onKeyDown: (e: KeyboardEvent<HTMLElement>) => void;
        onTouchStart: (e: TouchEvent<HTMLElement>) => void;
        onTouchMove: (e: TouchEvent<HTMLElement>) => void;
        onPointerDown: (e: PointerEvent<HTMLElement>) => void;
    };
    jumpToLatest: (smooth: boolean) => void;
    /** 程序要滚到中间某条（跳到被回复的消息）：先放开贴底，别被随后来的新条目拽回去 */
    detach: () => void;
    isFollowing: () => boolean;
    canPinToEnd: () => boolean;
}

export function useStickToBottom<T extends Keyed>(opts: {
    scrollRef: RefObject<HTMLDivElement | null>;
    virtualizer: Virtualizer<HTMLDivElement, Element>;
    items: readonly T[];
    /** debugScrollMemory 的键；null 表示不记 */
    memoryKey: string | null;
    /** 变了就当成换了一份列表（换会话、暂停后继续）：回到底部、清零计数 */
    resetToken: string;
    /** 筛选条件变了：列表内容变了但不是「新来的」，不计数 */
    filterToken: string;
    /** 允许进场动画（动画设置打开且不是减少动画） */
    animate: boolean;
    initialDetached?: boolean;
    reattachOnIntent?: boolean;
    followUntilUserScroll?: boolean;
}): StickToBottom {
    const { scrollRef, virtualizer, items, memoryKey, resetToken, filterToken, animate } = opts;

    // 记着位置（之前翻上去过）就从那里开始，否则从底部开始
    const stickRef = useRef<boolean>(
        !opts.initialDetached && (memoryKey === null || recallScroll(memoryKey) === undefined),
    );
    const downwardIntent = useRef(false);
    /** 用户明确往上翻了：几何上还在 120px 以内也不贴底 */
    const detachedRef = useRef(!stickRef.current);
    const autoRef = useRef(false);
    const [unseen, setUnseen] = useState(0);
    const [away, setAway] = useState(false);
    const lastKeyRef = useRef<string | null>(null);
    const mountedRef = useRef(false);
    const resetRef = useRef(resetToken);
    const filterRef = useRef(filterToken);
    const pendingEnter = useRef(new Set<string>());
    /** 这一轮提交里已经播过进场的行 */
    const playedEnter = useRef(new Set<string>());
    const lastTopRef = useRef(0);
    const touchYRef = useRef<number | null>(null);

    useScrollMemory(memoryKey, scrollRef, { shouldRemember });

    const onScroll = useCallback(() => {
        const el = scrollRef.current;
        if (!el) return;
        const d = distanceFromBottom(el);
        const top = el.scrollTop;
        const movedDown = top > lastTopRef.current + 0.5;
        lastTopRef.current = top;
        if (autoRef.current) {
            // 自动滚动途中只认「到底了」
            if (d > 2) return;
            autoRef.current = false;
        }
        // 放开之后：滚到最底，或者自己往下滚回 120px 以内，才重新贴底
        if (
            detachedRef.current &&
            (!opts.reattachOnIntent || downwardIntent.current) &&
            (d <= REATTACH_PX || (movedDown && d < STICK_PX))
        )
            detachedRef.current = false;
        const stuck =
            !detachedRef.current &&
            ((opts.followUntilUserScroll && stickRef.current) || d < STICK_PX);
        stickRef.current = stuck;
        if (stuck) setUnseen(0);
        setAway(!stuck && d > el.clientHeight);
    }, [scrollRef, opts.reattachOnIntent, opts.followUntilUserScroll]);

    // 平滑滚到底：不用 virtualizer 自带的 smooth——它在平滑滚动途中不量路过的行，
    // 滚得远时中途的行会叠在一起，没滚到头就停下的话一直叠着。这里远的先瞬移到离底一屏，
    // 最后一屏交给浏览器原生平滑滚动（途中照常量行高），滚完（或超时）再按实际位置钉到底。
    const autoTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
    const clearAutoTimer = useCallback(() => {
        if (autoTimer.current !== null) clearTimeout(autoTimer.current);
        autoTimer.current = null;
    }, []);
    useEffect(() => clearAutoTimer, [clearAutoTimer]);

    const snapToEnd = useCallback(() => {
        if (opts.followUntilUserScroll) {
            const element = scrollRef.current;
            if (element)
                element.scrollTop = Math.max(0, element.scrollHeight - element.clientHeight);
            return;
        }
        const n = virtualizer.options.count;
        if (n > 0) virtualizer.scrollToIndex(n - 1, { align: 'end' });
    }, [virtualizer, scrollRef, opts.followUntilUserScroll]);

    const finishAuto = useCallback(() => {
        clearAutoTimer();
        if (!autoRef.current) return;
        autoRef.current = false;
        // 平滑滚动途中行高又变了、没落到最底：用户没动过，就直接钉到底
        stickRef.current = true;
        detachedRef.current = false;
        snapToEnd();
    }, [snapToEnd, clearAutoTimer]);

    const scrollToEnd = useCallback(
        (smooth: boolean) => {
            const el = scrollRef.current;
            if (virtualizer.options.count === 0 || !el) return;
            if (!smooth) {
                snapToEnd();
                return;
            }
            const view = el.clientHeight;
            if (distanceFromBottom(el) > view * 1.5)
                el.scrollTop = Math.max(0, el.scrollHeight - view * 2);
            autoRef.current = true;
            el.scrollTo({ top: el.scrollHeight, behavior: 'smooth' });
            clearAutoTimer();
            autoTimer.current = setTimeout(finishAuto, AUTO_SCROLL_GRACE_MS);
        },
        [virtualizer, scrollRef, snapToEnd, finishAuto, clearAutoTimer],
    );

    // 浏览器报「滚完了」就收尾，不必等超时
    useEffect(() => {
        const el = scrollRef.current;
        if (!el) return;
        const onEnd = () => {
            if (autoRef.current) finishAuto();
        };
        el.addEventListener('scrollend', onEnd);
        return () => el.removeEventListener('scrollend', onEnd);
    }, [scrollRef, finishAuto]);

    // 渲染时算：这次提交里从哪一条开始是新来的（只读 ref）
    let enterFrom = Infinity;
    const sameList = resetRef.current === resetToken && filterRef.current === filterToken;
    if (animate && mountedRef.current && stickRef.current && sameList) {
        const appended = appendedAfter(items, lastKeyRef.current);
        if (appended > 0 && appended <= ENTER_MAX_BATCH) enterFrom = items.length - appended;
    }

    useLayoutEffect(() => {
        const last = items.length > 0 ? (items[items.length - 1]?.key ?? null) : null;
        const prevLast = lastKeyRef.current;
        lastKeyRef.current = last;
        // 行的挂载 effect 先于这里跑：这一轮挂上并播过的行都在 playedEnter 里，用完就清
        const played = playedEnter.current;
        playedEnter.current = new Set();

        if (!mountedRef.current) {
            mountedRef.current = true;
            if (stickRef.current) scrollToEnd(false);
            return;
        }

        if (resetRef.current !== resetToken) {
            resetRef.current = resetToken;
            filterRef.current = filterToken;
            pendingEnter.current.clear();
            stickRef.current = true;
            detachedRef.current = false;
            autoRef.current = false;
            if (memoryKey) forgetScroll(memoryKey);
            setUnseen(0);
            setAway(false);
            scrollToEnd(false);
            return;
        }

        const filterChanged = filterRef.current !== filterToken;
        filterRef.current = filterToken;
        if (last === prevLast && !filterChanged) return;

        if (stickRef.current) {
            pendingEnter.current.clear();
            if (!filterChanged && animate) {
                const appended = appendedAfter(items, prevLast);
                if (appended > 0 && appended <= ENTER_MAX_BATCH) {
                    for (let i = items.length - appended; i < items.length; i += 1) {
                        const key = items[i]?.key;
                        if (key && !played.has(key)) pendingEnter.current.add(key);
                    }
                }
            }
            // 正在平滑滚过去的途中又来了新的：接着平滑滚，别一下跳过去
            scrollToEnd(autoRef.current);
            return;
        }
        if (filterChanged) return;
        const appended = appendedAfter(items, prevLast);
        if (appended > 0) setUnseen((n) => n + appended);
    }, [items, resetToken, filterToken, memoryKey, animate, scrollToEnd]);

    // 原生会话等这一帧行高提交后贴底；虚拟器的滚动重试不能继续接管用户上翻。
    useLayoutEffect(() => {
        if (opts.followUntilUserScroll && stickRef.current && !autoRef.current) snapToEnd();
    });

    // 从记住的位置开始时：内容可能已经不够长、恢复后其实就在底部，下一帧按实际位置再判一次
    useLayoutEffect(() => {
        if (stickRef.current) return;
        const frame = requestAnimationFrame(onScroll);
        return () => cancelAnimationFrame(frame);
        // 只在挂上时
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, []);

    /** 用户自己动了滚动：打断正在进行的自动滚动；往上、而且真能往上滚的话，连贴底一起放开 */
    const userIntent = useCallback(
        (up: boolean) => {
            downwardIntent.current = !up;
            autoRef.current = false;
            clearAutoTimer();
            if (!up) return;
            const el = scrollRef.current;
            if (!el || el.scrollTop <= 0 || el.scrollHeight <= el.clientHeight) return;
            detachedRef.current = true;
            stickRef.current = false;
            pendingEnter.current.clear();
        },
        [clearAutoTimer, scrollRef],
    );

    const handlers = useMemo<StickToBottom['handlers']>(
        () => ({
            onScroll,
            onWheel: (e) => userIntent(e.deltaY < 0),
            onKeyDown: (e) => {
                if (UP_KEYS.has(e.key) || (e.key === ' ' && e.shiftKey)) userIntent(true);
                else if (DOWN_KEYS.has(e.key) || e.key === ' ') userIntent(false);
            },
            onTouchStart: (e) => {
                touchYRef.current = e.touches[0]?.clientY ?? null;
                userIntent(false);
            },
            onTouchMove: (e) => {
                const y = e.touches[0]?.clientY;
                const from = touchYRef.current;
                if (y === undefined || from === null) return;
                // 手指往下拖 = 内容往上翻
                if (y - from > 2) userIntent(true);
                touchYRef.current = y;
            },
            onPointerDown: (e) => {
                // 按在滚动条上（事件落在容器自己身上）：拖的方向看不出来，先放开，拖回底部会自己贴回去
                if (e.target === e.currentTarget) userIntent(true);
            },
        }),
        [onScroll, userIntent],
    );

    const jumpToLatest = useCallback(
        (smooth: boolean) => {
            stickRef.current = true;
            detachedRef.current = false;
            setUnseen(0);
            setAway(false);
            if (memoryKey) forgetScroll(memoryKey);
            scrollToEnd(smooth);
        },
        [memoryKey, scrollToEnd],
    );

    const detach = useCallback(() => {
        downwardIntent.current = false;
        stickRef.current = false;
        detachedRef.current = true;
        autoRef.current = false;
        clearAutoTimer();
        pendingEnter.current.clear();
    }, [clearAutoTimer]);

    const takeEnter = useCallback((key: string) => {
        playedEnter.current.add(key);
        return pendingEnter.current.delete(key);
    }, []);

    const isFollowing = useCallback(() => stickRef.current, []);
    const canPinToEnd = useCallback(() => stickRef.current && !autoRef.current, []);
    return {
        unseen,
        away,
        enterFrom,
        takeEnter,
        handlers,
        jumpToLatest,
        detach,
        isFollowing,
        canPinToEnd,
    };
}
