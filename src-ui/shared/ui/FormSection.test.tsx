import { createRef, useState } from 'react';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import { preferencesStore } from '../../core/domain/settings/preferencesStore';
import { FormSection } from './FormSection';

afterEach(() => {
    cleanup();
    preferencesStore.reset();
});

describe('表单分组内容显隐', () => {
    it('首次展开才挂载编辑器，收起后保留标题、操作和未保存草稿', () => {
        preferencesStore.setMotionEnabled(false);
        const ref = createRef<HTMLElement>();
        let mounts = 0;
        function Draft() {
            const [value, setValue] = useState(() => {
                mounts += 1;
                return '';
            });
            return (
                <input
                    aria-label="模型草稿"
                    value={value}
                    onChange={(e) => setValue(e.target.value)}
                />
            );
        }
        const view = (visible: boolean) => (
            <FormSection
                ref={ref}
                id="model-settings"
                title="模型设置"
                actions={<button type="button">恢复默认</button>}
                layout="grid-2"
                contentVisible={visible}
            >
                <Draft />
            </FormSection>
        );
        const { rerender } = render(view(false));
        expect(mounts).toBe(0);
        expect(screen.queryByLabelText('模型草稿')).not.toBeInTheDocument();
        expect(screen.getByRole('heading', { name: '模型设置' })).toBeVisible();
        expect(ref.current).toHaveAttribute('id', 'model-settings');
        expect(ref.current).not.toHaveAttribute('contentVisible');

        rerender(view(true));
        fireEvent.change(screen.getByRole('textbox', { name: '模型草稿' }), {
            target: { value: '尚未保存的模型配置' },
        });
        rerender(view(false));
        expect(screen.queryByRole('textbox', { name: '模型草稿' })).not.toBeInTheDocument();
        expect(screen.getByRole('heading', { name: '模型设置' })).toBeVisible();
        expect(screen.getByRole('button', { name: '恢复默认' })).toBeVisible();

        rerender(view(true));
        expect(screen.getByRole('textbox', { name: '模型草稿' })).toHaveValue('尚未保存的模型配置');
        expect(mounts).toBe(1);
    });
});
