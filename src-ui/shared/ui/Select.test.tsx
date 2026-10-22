import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import gsap from 'gsap';
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import { preferencesStore } from '../../core/domain/settings/preferencesStore';
import { Select } from './Select';

const ITEMS = [
    { value: 'a', label: '甲' },
    { value: 'b', label: '乙' },
];

function renderSelect() {
    return render(<Select items={ITEMS} value="a" onValueChange={() => {}} />);
}

// Radix Select 打开时会用到这几个 jsdom 没有的 DOM API
beforeAll(() => {
    Element.prototype.hasPointerCapture ??= () => false;
    Element.prototype.releasePointerCapture ??= () => {};
    Element.prototype.scrollIntoView ??= () => {};
});

afterEach(() => {
    preferencesStore.reset();
    vi.restoreAllMocks();
});

describe('Select 弹出层进场', () => {
    it('一次打开只进场一次：Radix 内部重渲、父级重渲都不重播', async () => {
        const user = userEvent.setup();
        const fromTo = vi.spyOn(gsap, 'fromTo');
        const { rerender } = renderSelect();

        await user.click(screen.getByRole('combobox'));
        const listbox = await screen.findByRole('listbox');
        rerender(<Select items={ITEMS} value="a" onValueChange={() => {}} />);

        expect(fromTo.mock.calls.filter(([target]) => target === listbox)).toHaveLength(1);
    });

    it('关掉再打开是新的内容节点，照常进场', async () => {
        const user = userEvent.setup();
        const fromTo = vi.spyOn(gsap, 'fromTo');
        renderSelect();

        await user.click(screen.getByRole('combobox'));
        const first = await screen.findByRole('listbox');
        await user.keyboard('{Escape}');
        await user.click(screen.getByRole('combobox'));
        const second = await screen.findByRole('listbox');

        expect(second).not.toBe(first);
        expect(fromTo.mock.calls.filter(([target]) => target === second)).toHaveLength(1);
    });

    it('关掉动效时直接可见，不跑进场动画', async () => {
        const user = userEvent.setup();
        preferencesStore.setMotionEnabled(false);
        const fromTo = vi.spyOn(gsap, 'fromTo');
        renderSelect();

        await user.click(screen.getByRole('combobox'));
        const listbox = await screen.findByRole('listbox');

        expect(listbox.style.visibility).not.toBe('hidden');
        expect(fromTo.mock.calls.filter(([target]) => target === listbox)).toHaveLength(0);
    });
});

describe('Select 标签关联', () => {
    it('未传 id 或 name 时，可见标签仍是稳定的可访问名称', () => {
        const label = <span title="模型思考强度">思考强度</span>;
        const { rerender } = render(
            <Select label={label} items={ITEMS} value="a" onValueChange={() => {}} />,
        );
        const trigger = screen.getByRole('combobox', { name: '思考强度' });
        const fieldId = trigger.id;
        const visibleLabel = screen.getByText('思考强度').closest('label');
        expect(fieldId).not.toBe('');
        expect(visibleLabel).toHaveAttribute('for', fieldId);
        expect(trigger).toHaveAttribute('aria-labelledby', visibleLabel?.id);
        expect(trigger).toHaveTextContent('甲');

        rerender(<Select label={label} items={ITEMS} value="b" onValueChange={() => {}} />);
        expect(screen.getByRole('combobox', { name: '思考强度' })).toBe(trigger);
        expect(trigger).toHaveAttribute('id', fieldId);
        expect(trigger).toHaveTextContent('乙');
    });

    it('多份未指定 id 的控件各自关联自己的标签与提示', () => {
        render(
            <>
                <Select
                    label="类别"
                    hint="留空时使用默认类别"
                    items={ITEMS}
                    value="a"
                    onValueChange={() => {}}
                />
                <Select
                    label="协议"
                    hint="选择连接使用的协议"
                    items={ITEMS}
                    value="b"
                    onValueChange={() => {}}
                />
            </>,
        );
        const category = screen.getByRole('combobox', { name: '类别' });
        const protocol = screen.getByRole('combobox', { name: '协议' });
        expect(category.id).not.toBe(protocol.id);
        expect(category).toHaveAccessibleDescription('留空时使用默认类别');
        expect(protocol).toHaveAccessibleDescription('选择连接使用的协议');
    });

    it('无可见标签时保留显式 aria-label，且不生成无效标签引用', () => {
        const { rerender } = render(
            <Select
                aria-label="重启频率"
                items={ITEMS}
                value="a"
                onValueChange={() => {}}
            />,
        );
        const trigger = screen.getByRole('combobox', { name: '重启频率' });
        expect(trigger).not.toHaveAttribute('aria-labelledby');

        rerender(<Select items={ITEMS} value="a" onValueChange={() => {}} />);
        expect(screen.getByRole('combobox', { name: '甲' })).toBe(trigger);
        expect(trigger).not.toHaveAttribute('aria-labelledby');
    });

    it('保留显式 id 和 name 的关联方式', () => {
        render(
            <Select
                id="model-provider"
                name="provider"
                label="提供商"
                items={ITEMS}
                value="a"
                onValueChange={() => {}}
            />,
        );
        const trigger = screen.getByRole('combobox', { name: '提供商' });
        expect(trigger).toHaveAttribute('id', 'model-provider');
        expect(screen.getByText('提供商')).toHaveAttribute('for', 'model-provider');
    });
});
