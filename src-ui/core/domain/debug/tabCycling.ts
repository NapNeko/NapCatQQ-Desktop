// 标签页 / 面板组的循环切换：这里只算「下一格」，落到 store 还是焦点由调用方决定。
// index 允许传 -1（当前项不在列表里，比如活动标签被外部关掉了）：+1 时从列表头开始，-1 时从列表尾开始。

/** 环形列表的下一个下标；step 只有前进 / 后退两档 */
export function nextIndex(index: number, length: number, step: 1 | -1): number {
    return (index + step + length) % length;
}

/** 当前活动标签的下一个 / 上一个标签 id；不足两个标签返回 null（没什么可切的） */
export function nextTabId(
    tabs: readonly { id: string }[],
    activeTab: string | null,
    dir: 1 | -1,
): string | null {
    if (tabs.length < 2) return null;
    const idx = Math.max(
        0,
        tabs.findIndex((t) => t.id === activeTab),
    );
    const next = tabs[nextIndex(idx, tabs.length, dir)];
    return next ? next.id : null;
}
