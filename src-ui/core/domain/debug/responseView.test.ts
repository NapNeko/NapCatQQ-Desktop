import { describe, expect, it } from 'vitest';
import {
    copyAllResponseCopy,
    formatBytes,
    isClickableId,
    responseFileName,
    tableView,
} from './responseView';

describe('tableView', () => {
    it('对象数组：列是键的并集，按第一次出现的先后', () => {
        const rows = [
            { group_id: 1, group_name: 'A' },
            { group_id: 2, member_count: 30 },
            { extra: true, group_id: 3 },
        ];
        const view = tableView(rows);
        expect(view?.columns).toEqual(['group_id', 'group_name', 'member_count', 'extra']);
        expect(view?.rows).toBe(rows);
    });

    it('不是「至少一个元素、全是对象」的数组就不可用', () => {
        expect(tableView([])).toBeNull();
        expect(tableView(null)).toBeNull();
        expect(tableView(undefined)).toBeNull();
        expect(tableView({ a: 1 })).toBeNull();
        expect(tableView('str')).toBeNull();
        expect(tableView([1, 2, 3])).toBeNull();
        expect(tableView([{ a: 1 }, 2])).toBeNull();
        expect(tableView([{ a: 1 }, null])).toBeNull();
        expect(tableView([{ a: 1 }, [1]])).toBeNull();
    });

    it('单个空对象组成的数组：可用，但没有列', () => {
        expect(tableView([{}])).toEqual({ columns: [], rows: [{}] });
    });

    it('最多 30 列', () => {
        const wide = Object.fromEntries(Array.from({ length: 45 }, (_, i) => [`k${i}`, i]));
        const view = tableView([wide, { late: 1 }]);
        expect(view?.columns).toHaveLength(30);
        expect(view?.columns[0]).toBe('k0');
        expect(view?.columns[29]).toBe('k29');
        expect(view?.columns).not.toContain('late');
    });
});

describe('formatBytes', () => {
    it('各量级', () => {
        expect(formatBytes(0)).toBe('0 B');
        expect(formatBytes(512)).toBe('512 B');
        expect(formatBytes(1023)).toBe('1023 B');
        expect(formatBytes(1024)).toBe('1 KB');
        expect(formatBytes(1536)).toBe('1.5 KB');
        expect(formatBytes(256 * 1024)).toBe('256 KB');
        expect(formatBytes(5 * 1024 * 1024)).toBe('5 MB');
        expect(formatBytes(1.5 * 1024 * 1024 * 1024)).toBe('1.5 GB');
        expect(formatBytes(3 * 1024 ** 4)).toBe('3 TB');
        expect(formatBytes(5000 * 1024 ** 4)).toBe('5000 TB');
    });

    it('异常输入当 0', () => {
        expect(formatBytes(-5)).toBe('0 B');
        expect(formatBytes(Number.NaN)).toBe('0 B');
        expect(formatBytes(Number.POSITIVE_INFINITY)).toBe('0 B');
    });
});

describe('isClickableId', () => {
    it('三种 id 键可点，其它不行', () => {
        expect(isClickableId('group_id')).toBe('group_id');
        expect(isClickableId('user_id')).toBe('user_id');
        expect(isClickableId('message_id')).toBe('message_id');
        expect(isClickableId('nickname')).toBeNull();
        expect(isClickableId('Group_Id')).toBeNull();
        expect(isClickableId('')).toBeNull();
    });
});

describe('copyAllResponseCopy', () => {
    it('截断时只能说复制了开头一段，不能叫「完整回包」', () => {
        const copy = copyAllResponseCopy(true);
        expect(copy.label).toContain('开头 256 KiB');
        expect(copy.toast).toContain('开头 256 KiB');
        expect(copy.toast).toContain('另存');
        expect(copy.toast).not.toBe('已复制完整回包');
    });

    it('没截断保持「完整回包」', () => {
        expect(copyAllResponseCopy(false)).toEqual({
            label: '复制完整回包',
            toast: '已复制完整回包',
        });
    });
});

describe('responseFileName', () => {
    it('接口名 + 本地时间；奇怪的字符换掉，空名字用 response', () => {
        const at = new Date(2026, 8, 30, 7, 5, 9).getTime();
        expect(responseFileName('get_group_member_list', at)).toBe(
            'get_group_member_list-20260930-070509.json',
        );
        expect(responseFileName('a/b c', at)).toBe('a_b_c-20260930-070509.json');
        expect(responseFileName('  ', at)).toBe('response-20260930-070509.json');
    });
});
