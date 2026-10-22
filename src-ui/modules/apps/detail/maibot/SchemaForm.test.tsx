import { useState } from 'react';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeAll, beforeEach, expect, it, vi } from 'vitest';
import { preferencesStore } from '../../../../core/domain/settings/preferencesStore';
import type { UiNode } from '../../../../core/domain/apps/maibotSchema';
import { FieldGrid } from './SchemaForm';

beforeAll(() => {
    Element.prototype.hasPointerCapture ??= () => false;
    Element.prototype.releasePointerCapture ??= () => {};
    Element.prototype.scrollIntoView ??= () => {};
});

beforeEach(() => preferencesStore.setMotionEnabled(false));
afterEach(() => preferencesStore.reset());

function mount(node: UiNode, initial: Record<string, unknown>) {
    const changed = vi.fn();
    function Harness() {
        const [value, setValue] = useState<unknown>(initial);
        return (
            <FieldGrid
                node={node}
                path={[]}
                ctx={{
                    value,
                    onChange: (next) => {
                        changed(next);
                        setValue(next);
                    },
                    errors: {},
                    prefix: 'models',
                    showAdvanced: true,
                }}
            />
        );
    }
    render(<Harness />);
    return changed;
}

const mapSchema: UiNode = {
    fields: [{ name: 'services', type: 'object', mapValues: true, label: '联网服务' }],
    nested: {
        services: {
            fields: [{ name: 'api_key', type: 'string', label: '密钥' }],
        },
    },
};

it('服务名称草稿需显式应用，重复或空名称不能应用', async () => {
    const user = userEvent.setup();
    const changed = mount(mapSchema, {
        services: {
            first: { api_key: 'saved', future_field: true },
            second: { api_key: 'other' },
        },
    });
    const input = screen.getAllByRole('textbox', { name: '名称' })[0]!;
    await user.clear(input);
    await user.type(input, 'renamed');
    await user.tab();
    expect(changed).not.toHaveBeenCalled();

    await user.click(screen.getAllByRole('button', { name: '应用名称' })[0]!);
    expect(changed).toHaveBeenLastCalledWith({
        services: {
            renamed: { api_key: 'saved', future_field: true },
            second: { api_key: 'other' },
        },
    });
    changed.mockClear();

    const renamed = screen.getAllByRole('textbox', { name: '名称' })[0]!;
    await user.clear(renamed);
    await user.type(renamed, 'second');
    expect(screen.getAllByRole('button', { name: '应用名称' })[0]).toBeDisabled();
    await user.keyboard('{Enter}');
    expect(changed).not.toHaveBeenCalled();

    await user.clear(renamed);
    expect(screen.getAllByRole('button', { name: '应用名称' })[0]).toBeDisabled();
    expect(changed).not.toHaveBeenCalled();
});

it('服务名称可以按 Enter 应用', async () => {
    const user = userEvent.setup();
    const changed = mount(mapSchema, { services: { first: { api_key: 'saved' } } });
    const input = screen.getByRole('textbox', { name: '名称' });
    await user.clear(input);
    await user.type(input, 'renamed{Enter}');
    expect(changed).toHaveBeenLastCalledWith({
        services: { renamed: { api_key: 'saved' } },
    });
});

it('含空值的枚举可以打开并从具体值切回默认值', async () => {
    const user = userEvent.setup();
    const changed = mount(
        {
            fields: [
                {
                    name: 'category',
                    label: '类别',
                    type: 'select',
                    options: ['', 'programming', 'research'],
                },
            ],
        },
        { category: '' },
    );
    await user.click(screen.getByRole('combobox', { name: '类别' }));
    expect(await screen.findByRole('option', { name: '默认' })).toBeInTheDocument();
    await user.click(screen.getByRole('option', { name: 'programming' }));
    expect(changed).toHaveBeenLastCalledWith({ category: 'programming' });

    await user.click(screen.getByRole('combobox', { name: '类别' }));
    await user.click(await screen.findByRole('option', { name: '默认' }));
    expect(changed).toHaveBeenLastCalledWith({ category: '' });
});
