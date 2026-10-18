import { useState } from 'react';
import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { UiNode } from '../../../../core/domain/apps/maibotSchema';
import { FieldGrid, type SchemaCtx } from './SchemaForm';

vi.mock('../../../../hooks/preferences/useMotion', () => ({
    useMotion: () => ({ enabled: false, level: 'elegant', speed: 1 }),
}));

const NODE: UiNode = {
    fields: [
        { name: 'name', label: '名称', type: 'string' },
        { name: 'draft', type: 'string', advanced: true },
        { name: 'secret', label: '永不显示', type: 'string', hidden: true },
        { name: 'extra', type: 'object' },
    ],
    nested: {
        extra: {
            uiAdvanced: true,
            fields: [{ name: 'retry', label: '重试次数', type: 'integer' }],
        },
    },
};

function DraftWidget() {
    const [draft, setDraft] = useState('初始草稿');
    return (
        <input
            aria-label="高级草稿"
            value={draft}
            onChange={(event) => setDraft(event.target.value)}
        />
    );
}

const context = (showAdvanced: boolean): SchemaCtx => ({
    value: { sec: { name: '实例', extra: { retry: 3 } } },
    onChange: () => {},
    errors: {},
    prefix: 'bot',
    showAdvanced,
    widgets: { 'sec.draft': () => <DraftWidget /> },
});

describe('FieldGrid 高级选项', () => {
    it('初次收起时不创建高级编辑器，展开后保留草稿以便反复切换', () => {
        const tree = (open: boolean) => (
            <FieldGrid ctx={context(open)} path={['sec']} node={NODE} />
        );
        const view = render(tree(false));
        expect(screen.getByLabelText('名称')).toHaveValue('实例');
        expect(screen.queryByLabelText('高级草稿')).not.toBeInTheDocument();

        view.rerender(tree(true));
        fireEvent.change(screen.getByLabelText('高级草稿'), { target: { value: '未提交的编辑' } });
        expect(screen.getByLabelText('重试次数')).toHaveValue(3);
        expect(screen.queryByLabelText('永不显示')).not.toBeInTheDocument();

        view.rerender(tree(false));
        expect(screen.getByLabelText('高级草稿').closest('[aria-hidden="true"]')).not.toBeNull();
        view.rerender(tree(true));
        expect(screen.getByLabelText('高级草稿')).toHaveValue('未提交的编辑');
        expect(screen.getByLabelText('重试次数')).toHaveValue(3);
    });
});
