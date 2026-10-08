import { describe, expect, it } from 'vitest';
import { nextIndex, nextTabId } from './tabCycling';

const TABS = [{ id: 'a' }, { id: 'b' }, { id: 'c' }];

describe('nextIndex', () => {
    it('在两端回绕', () => {
        expect(nextIndex(0, 3, -1)).toBe(2);
        expect(nextIndex(2, 3, 1)).toBe(0);
        expect(nextIndex(1, 3, 1)).toBe(2);
    });

    it('未命中（-1）时 +1 从头部、-1 从尾部开始', () => {
        expect(nextIndex(-1, 3, 1)).toBe(0);
        expect(nextIndex(-1, 3, -1)).toBe(1);
    });
});

describe('nextTabId', () => {
    it('不足两个标签不给切', () => {
        expect(nextTabId([], null, 1)).toBeNull();
        expect(nextTabId([{ id: 'a' }], 'a', 1)).toBeNull();
    });

    it('活动标签在中间时按方向取邻居', () => {
        expect(nextTabId(TABS, 'b', 1)).toBe('c');
        expect(nextTabId(TABS, 'b', -1)).toBe('a');
    });

    it('两端回绕；活动 id 不在列表里（或为 null）时按下标 0 起算', () => {
        expect(nextTabId(TABS, 'c', 1)).toBe('a');
        // 原实现 Math.max(0, findIndex) 把未知活动 id 归到 0，-1 步环绕到尾部
        expect(nextTabId(TABS, null, -1)).toBe('c');
        expect(nextTabId(TABS, 'gone', 1)).toBe('b');
    });
});
