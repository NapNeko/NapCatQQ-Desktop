import { act, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createRef } from 'react';
import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest';
import { JsonTree, type JsonTreeHandle } from './JsonTree';

// jsdom 里所有元素的尺寸都是 0，虚拟列表会认为视口高度为 0 而一行都不画。
// 这里让树容器 440px（约 20 行）、行 22px，其余行为和真实浏览器一致。
const offsetHeight = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'offsetHeight');
const offsetWidth = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'offsetWidth');

beforeAll(() => {
    Object.defineProperty(HTMLElement.prototype, 'offsetHeight', {
        configurable: true,
        get() {
            return (this as HTMLElement).getAttribute('role') === 'tree' ? 440 : 22;
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

const SAMPLE = {
    name: 'napcat',
    items: [10, 20, { deep: true }],
    nested: { a: { b: 'x' } },
};

/** 按行文本开头找到某一行（键名 + 冒号 + 值）。 */
function rowStartingWith(prefix: string): HTMLElement {
    const row = screen.getAllByRole('treeitem').find((el) => el.textContent?.startsWith(prefix));
    if (!row) throw new Error(`找不到以 ${prefix} 开头的行`);
    return row;
}

describe('JsonTree 展开与收起', () => {
    it('默认展开两层；点容器行展开、再点收起', async () => {
        const user = userEvent.setup();
        render(<JsonTree value={SAMPLE} />);

        // 根和第一层的容器展开，第二层（items[2]、nested.a）默认收着
        expect(screen.getByText('"napcat"')).toBeInTheDocument();
        expect(screen.getByText('20')).toBeInTheDocument();
        expect(screen.queryByText('deep')).not.toBeInTheDocument();

        const row = rowStartingWith('2:{');
        expect(row).toHaveAttribute('aria-expanded', 'false');
        await user.click(row);
        expect(screen.getByText('deep')).toBeInTheDocument();
        expect(rowStartingWith('2:{')).toHaveAttribute('aria-expanded', 'true');

        await user.click(rowStartingWith('2:{'));
        expect(screen.queryByText('deep')).not.toBeInTheDocument();
    });

    it('defaultExpandDepth 控制默认展开的层数', () => {
        const { rerender } = render(<JsonTree value={SAMPLE} defaultExpandDepth={1} />);
        // 只展开根：第一层的容器只显示摘要
        expect(screen.getByText('"napcat"')).toBeInTheDocument();
        expect(screen.queryByText('20')).not.toBeInTheDocument();

        rerender(<JsonTree value={SAMPLE} defaultExpandDepth={3} />);
        expect(screen.getByText('deep')).toBeInTheDocument();
        expect(screen.getByText('"x"')).toBeInTheDocument();
    });

    it('全部展开 / 全部收起通过 handleRef 暴露', () => {
        const ref = createRef<JsonTreeHandle>();
        render(<JsonTree value={SAMPLE} handleRef={ref} />);
        expect(screen.queryByText('"x"')).not.toBeInTheDocument();

        act(() => ref.current?.expandAll());
        expect(screen.getByText('"x"')).toBeInTheDocument();
        expect(screen.getByText('deep')).toBeInTheDocument();

        act(() => ref.current?.collapseAll());
        // 收起后根仍展开、显示第一层（name、items、nested），更深的都收着
        expect(screen.getAllByRole('treeitem')).toHaveLength(4);
        expect(screen.queryByText('20')).not.toBeInTheDocument();
        expect(screen.queryByText('"x"')).not.toBeInTheDocument();
    });
});

describe('JsonTree 点击值', () => {
    it('onValueClick 带着正确的路径、键名和值', async () => {
        const user = userEvent.setup();
        const onValueClick = vi.fn();
        render(<JsonTree value={SAMPLE} defaultExpandDepth={3} onValueClick={onValueClick} />);

        await user.click(screen.getByText('"napcat"'));
        expect(onValueClick).toHaveBeenLastCalledWith({
            path: ['name'],
            key: 'name',
            value: 'napcat',
        });

        // 数组下标在路径里是数字
        await user.click(screen.getByText('20'));
        expect(onValueClick).toHaveBeenLastCalledWith({ path: ['items', 1], key: '1', value: 20 });

        await user.click(screen.getByText('true'));
        expect(onValueClick).toHaveBeenLastCalledWith({
            path: ['items', 2, 'deep'],
            key: 'deep',
            value: true,
        });
    });

    it('valueActions 渲染额外操作，并拿到该行的上下文', () => {
        const valueActions = vi.fn((ctx: { path: Array<string | number> }) => (
            <button type="button">用作参数 {ctx.path.join('.')}</button>
        ));
        render(<JsonTree value={{ id: 7 }} valueActions={valueActions} />);
        expect(screen.getByRole('button', { name: '用作参数 id' })).toBeInTheDocument();
    });
});

describe('JsonTree 键盘', () => {
    it('方向键移动、左右收起展开、回车触发叶子的值点击', async () => {
        const user = userEvent.setup();
        const onValueClick = vi.fn();
        render(<JsonTree value={SAMPLE} onValueClick={onValueClick} />);

        await user.tab();
        expect(screen.getByRole('tree')).toHaveFocus();

        // 落在根上，← 收起根
        await user.keyboard('{ArrowLeft}');
        expect(screen.getAllByRole('treeitem')).toHaveLength(1);
        // → 再展开，回到原样
        await user.keyboard('{ArrowRight}');
        expect(screen.getByText('"napcat"')).toBeInTheDocument();

        // ↓ 落到 name，回车等价于点它的值
        await user.keyboard('{ArrowDown}{Enter}');
        expect(onValueClick).toHaveBeenLastCalledWith({
            path: ['name'],
            key: 'name',
            value: 'napcat',
        });
        expect(rowStartingWith('name')).toHaveAttribute('aria-selected', 'true');

        // 往下走到 items，回车切换展开
        await user.keyboard('{ArrowDown}');
        expect(rowStartingWith('items')).toHaveAttribute('aria-expanded', 'true');
        await user.keyboard('{Enter}');
        expect(rowStartingWith('items')).toHaveAttribute('aria-expanded', 'false');
        expect(screen.queryByText('20')).not.toBeInTheDocument();
    });
});

describe('JsonTree 键盘：Tab 顺序、复制、选中项', () => {
    it('行里的按钮不进 Tab 顺序：Tab 一下进树，再一下就出树', async () => {
        const user = userEvent.setup();
        render(
            <>
                <JsonTree value={{ s: 'x'.repeat(300), n: 1 }} />
                <button type="button">树外按钮</button>
            </>,
        );
        for (const b of [
            ...screen.getAllByRole('button', { name: /复制/ }),
            screen.getByRole('button', { name: '展开' }),
        ]) {
            expect(b).toHaveAttribute('tabindex', '-1');
        }

        await user.tab();
        expect(screen.getByRole('tree')).toHaveFocus();
        await user.tab();
        expect(screen.getByRole('button', { name: '树外按钮' })).toHaveFocus();
    });

    it('按 c 复制选中节点的 JSON，带修饰键的 c 不拦', async () => {
        const user = userEvent.setup();
        render(<JsonTree value={SAMPLE} />);

        await user.tab();
        // 根 → name → items
        await user.keyboard('{ArrowDown}{ArrowDown}c');
        expect(await navigator.clipboard.readText()).toBe(JSON.stringify(SAMPLE.items, null, 2));
        // 复制后行内按钮变成「已复制」
        expect(
            within(rowStartingWith('items')).getByRole('button', { name: '已复制' }),
        ).toBeInTheDocument();

        await user.keyboard('{ArrowUp}{Control>}c{/Control}');
        expect(await navigator.clipboard.readText()).toBe(JSON.stringify(SAMPLE.items, null, 2));
    });

    it('选中行被折进祖先里后，选中项落到那个祖先，方向键从它接着走', async () => {
        const user = userEvent.setup();
        const ref = createRef<JsonTreeHandle>();
        render(<JsonTree value={SAMPLE} handleRef={ref} />);

        // 点 items[1] 让它成为选中行，再整体收起
        await user.click(screen.getByText('20'));
        expect(rowStartingWith('1:20')).toHaveAttribute('aria-selected', 'true');
        act(() => ref.current?.collapseAll());

        expect(rowStartingWith('items')).toHaveAttribute('aria-selected', 'true');
        // 点击时焦点已经落在树上，直接按方向键
        await user.keyboard('{ArrowDown}');
        expect(rowStartingWith('nested')).toHaveAttribute('aria-selected', 'true');
    });
});

describe('JsonTree 长字符串', () => {
    it('超过 200 字符先截断，点「展开」看全文、点「收起」还原', async () => {
        const user = userEvent.setup();
        const long = 'x'.repeat(300);
        render(<JsonTree value={{ s: long }} />);

        expect(screen.queryByText(new RegExp('x{300}'))).not.toBeInTheDocument();
        await user.click(screen.getByRole('button', { name: '展开' }));
        expect(screen.getByText(new RegExp('x{300}'))).toBeInTheDocument();

        await user.click(screen.getByRole('button', { name: '收起' }));
        expect(screen.queryByText(new RegExp('x{300}'))).not.toBeInTheDocument();
    });
});

describe('JsonTree 虚拟化', () => {
    it('1 万项的数组只画一个窗口的行，总高度仍按全部行算', () => {
        const big = Array.from({ length: 10_000 }, (_, i) => i);
        render(<JsonTree value={big} />);

        const rendered = screen.getAllByRole('treeitem').length;
        expect(rendered).toBeGreaterThan(0);
        // 视口约 20 行，加上前后 overscan，远小于 1 万
        expect(rendered).toBeLessThan(80);

        // 根 + 1 万个元素，每行 22px
        const inner = screen.getByRole('tree').firstElementChild as HTMLElement;
        expect(inner.style.height).toBe(`${10_001 * 22}px`);
    });

    it('5 万行的大数组也能一次算出行来', () => {
        const big = Array.from({ length: 50_000 }, (_, i) => ({ id: i, name: `n${i}` }));
        const start = performance.now();
        render(<JsonTree value={big} />);
        // 元素数量太多时，默认展开预算会让元素保持收起，不会生成 10 万+ 行
        expect(screen.getAllByRole('treeitem').length).toBeLessThan(80);
        expect(performance.now() - start).toBeLessThan(2000);
        const inner = screen.getByRole('tree').firstElementChild as HTMLElement;
        expect(inner.style.height).toBe(`${50_001 * 22}px`);
    });
});
