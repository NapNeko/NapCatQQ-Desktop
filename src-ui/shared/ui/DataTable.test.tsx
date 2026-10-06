import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest';
import { DataTable } from './DataTable';

// jsdom 里元素尺寸全是 0，虚拟列表不画任何行；让表格滚动区有 300px 高。
const offsetHeight = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'offsetHeight');
const offsetWidth = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'offsetWidth');

beforeAll(() => {
    Object.defineProperty(HTMLElement.prototype, 'offsetHeight', {
        configurable: true,
        get() {
            return (this as HTMLElement).getAttribute('role') === 'table' ? 300 : 28;
        },
    });
    Object.defineProperty(HTMLElement.prototype, 'offsetWidth', {
        configurable: true,
        get: () => 600,
    });
});

afterAll(() => {
    if (offsetHeight) Object.defineProperty(HTMLElement.prototype, 'offsetHeight', offsetHeight);
    if (offsetWidth) Object.defineProperty(HTMLElement.prototype, 'offsetWidth', offsetWidth);
});

const COLUMNS = ['name', 'score'];
const ROWS = [
    { name: 'alice', score: 9 },
    { name: 'bob', score: 100 },
    { name: 'carol', score: 10 },
    { name: 'dave', score: 2 },
];

/** 数据行（不含表头）第一列的文本，按显示顺序。 */
function firstColumn(): string[] {
    return screen
        .getAllByRole('row')
        .slice(1)
        .map((row) => within(row).getAllByRole('cell')[0]!.textContent ?? '');
}

describe('DataTable 排序', () => {
    it('数字列按数值排，不是按字符串：2 < 9 < 10 < 100', async () => {
        const user = userEvent.setup();
        render(<DataTable columns={COLUMNS} rows={ROWS} />);
        expect(firstColumn()).toEqual(['alice', 'bob', 'carol', 'dave']);

        await user.click(screen.getByRole('button', { name: 'score' }));
        expect(firstColumn()).toEqual(['dave', 'alice', 'carol', 'bob']);
        expect(screen.getByRole('columnheader', { name: 'score' })).toHaveAttribute(
            'aria-sort',
            'ascending',
        );

        // 再点一次降序
        await user.click(screen.getByRole('button', { name: 'score' }));
        expect(firstColumn()).toEqual(['bob', 'carol', 'alice', 'dave']);
        expect(screen.getByRole('columnheader', { name: 'score' })).toHaveAttribute(
            'aria-sort',
            'descending',
        );

        // 第三次还原成原顺序
        await user.click(screen.getByRole('button', { name: 'score' }));
        expect(firstColumn()).toEqual(['alice', 'bob', 'carol', 'dave']);
        expect(screen.getByRole('columnheader', { name: 'score' })).toHaveAttribute(
            'aria-sort',
            'none',
        );
    });

    it('存成字符串的数字也按数值排；空值不论升降都在最后', async () => {
        const user = userEvent.setup();
        const rows = [{ id: '100' }, { id: null }, { id: '9' }, { id: '20' }];
        render(<DataTable columns={['id']} rows={rows} />);

        await user.click(screen.getByRole('button', { name: 'id' }));
        expect(firstColumn()).toEqual(['9', '20', '100', 'null']);
        await user.click(screen.getByRole('button', { name: 'id' }));
        expect(firstColumn()).toEqual(['100', '20', '9', 'null']);
    });

    it('文本列用自然序：群 2 排在群 10 前面', async () => {
        const user = userEvent.setup();
        render(
            <DataTable
                columns={['name']}
                rows={[{ name: '群 10' }, { name: '群 2' }, { name: '群 1' }]}
            />,
        );
        await user.click(screen.getByRole('button', { name: 'name' }));
        expect(firstColumn()).toEqual(['群 1', '群 2', '群 10']);
    });
});

describe('DataTable 过滤与渲染', () => {
    it('过滤是对所有列做子串匹配，不分大小写', async () => {
        const user = userEvent.setup();
        render(<DataTable columns={COLUMNS} rows={ROWS} />);

        await user.type(screen.getByRole('searchbox', { name: '过滤表格行' }), 'CAR');
        expect(firstColumn()).toEqual(['carol']);
        expect(screen.getByText('1 / 4 行')).toBeInTheDocument();

        // 命中的是别的列（分数）也算
        await user.clear(screen.getByRole('searchbox'));
        await user.type(screen.getByRole('searchbox'), '100');
        expect(firstColumn()).toEqual(['bob']);

        await user.clear(screen.getByRole('searchbox'));
        await user.type(screen.getByRole('searchbox'), 'zzz');
        expect(screen.getByText('没有匹配的行')).toBeInTheDocument();
    });

    it('对象格子压成单行 JSON，并带 title', () => {
        render(<DataTable columns={['meta']} rows={[{ meta: { a: 1, b: [2, 3] } }]} />);
        const cell = screen.getByRole('cell');
        expect(cell).toHaveTextContent('{"a":1,"b":[2,3]}');
        expect(cell).toHaveAttribute('title', '{"a":1,"b":[2,3]}');
    });

    it('点单元格带出整行、列名和值', async () => {
        const user = userEvent.setup();
        const onCellClick = vi.fn();
        render(<DataTable columns={COLUMNS} rows={ROWS} onCellClick={onCellClick} />);

        await user.click(screen.getByText('bob'));
        expect(onCellClick).toHaveBeenLastCalledWith({
            row: ROWS[1],
            column: 'name',
            value: 'bob',
        });
        await user.click(screen.getByText('100'));
        expect(onCellClick).toHaveBeenLastCalledWith({ row: ROWS[1], column: 'score', value: 100 });
    });

    it('可点的格子能用键盘到达，回车触发；没传 onCellClick 时格子不占 Tab 位', async () => {
        const user = userEvent.setup();
        const onCellClick = vi.fn();
        const { unmount } = render(
            <DataTable columns={COLUMNS} rows={ROWS} onCellClick={onCellClick} />,
        );

        const cell = screen.getByText('bob');
        expect(cell).toHaveAttribute('tabindex', '0');
        cell.focus();
        await user.keyboard('{Enter}');
        expect(onCellClick).toHaveBeenCalledWith({ row: ROWS[1], column: 'name', value: 'bob' });
        unmount();

        render(<DataTable columns={COLUMNS} rows={ROWS} />);
        expect(screen.getByText('bob')).not.toHaveAttribute('tabindex');
    });

    it('没有数据时给出提示', () => {
        render(<DataTable columns={COLUMNS} rows={[]} />);
        expect(screen.getByText('没有数据')).toBeInTheDocument();
    });

    it('1 万行只画一个窗口的行', () => {
        const rows = Array.from({ length: 10_000 }, (_, i) => ({ name: `n${i}`, score: i }));
        render(<DataTable columns={COLUMNS} rows={rows} />);
        const dataRows = screen.getAllByRole('row').length - 1;
        expect(dataRows).toBeGreaterThan(0);
        expect(dataRows).toBeLessThan(60);
        expect(screen.getByText('10000 行')).toBeInTheDocument();
    });
});
