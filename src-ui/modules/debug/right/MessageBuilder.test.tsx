import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { TooltipProvider } from '../../../shared/ui';
import { preferencesStore } from '../../../hooks/preferences/preferencesStore';
import type { ComposerEntry } from '../../../core/domain/debug/messageBuilder';
import { MessageBuilderDialog } from './MessageBuilder';

// Radix Select 打开时会用到这几个 jsdom 没有的 DOM API
beforeAll(() => {
    Element.prototype.hasPointerCapture ??= () => false;
    Element.prototype.releasePointerCapture ??= () => {};
    Element.prototype.scrollIntoView ??= () => {};
});

beforeEach(() => {
    preferencesStore.setMotionEnabled(false);
});

afterEach(() => {
    cleanup();
    preferencesStore.reset();
});

function renderBuilder(entry: ComposerEntry, onApply = vi.fn(), replyId: number | null = null) {
    render(
        <TooltipProvider>
            <MessageBuilderDialog
                open
                onOpenChange={() => {}}
                entry={entry}
                replyId={replyId}
                onApply={onApply}
            />
        </TooltipProvider>,
    );
    return onApply;
}

const text = (t: string) => ({ type: 'text', data: { text: t } });
const image = (file: string) => ({ type: 'image', data: { file } });

/** 一行一个文本段时的输入框们，按文档顺序 */
const textboxes = () => screen.getAllByLabelText<HTMLTextAreaElement>('文字内容');

describe('MessageBuilder', () => {
    it('打开时把草稿解析成段列表：构建器段在前，手打文字在后', async () => {
        renderBuilder({
            text: '@阿强 你好',
            mentions: [{ qq: '10003', label: '@阿强' }],
            rich: [image('http://a/1.png')],
        });
        await screen.findByRole('dialog', { name: '消息构建器' });
        const img = screen.getByLabelText('图片地址');
        const at = screen.getByLabelText('QQ 号');
        const txt = screen.getByLabelText('文字内容');
        expect(img).toHaveValue('http://a/1.png');
        expect(at).toHaveValue('10003');
        expect(txt).toHaveValue(' 你好');
        expect(img.compareDocumentPosition(at) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
        expect(at.compareDocumentPosition(txt) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    });

    it('上移下移调整顺序、删段', async () => {
        renderBuilder({ text: '', mentions: [], rich: [text('甲'), text('乙'), text('丙')] });
        await screen.findByRole('dialog', { name: '消息构建器' });
        expect(textboxes().map((t) => t.value)).toEqual(['甲', '乙', '丙']);

        // 第一行下移 → 乙 甲 丙；顶上的行没有上移、最底的行没有下移（按钮置灰）
        await userEvent
            .setup()
            .click(screen.getAllByRole('button', { name: '下移' })[0] as HTMLElement);
        expect(textboxes().map((t) => t.value)).toEqual(['乙', '甲', '丙']);
        expect(screen.getAllByRole('button', { name: '上移' })[0]).toBeDisabled();
        expect(screen.getAllByRole('button', { name: '下移' })[2]).toBeDisabled();

        // 删掉「甲」那一行（现在排第二）
        await userEvent
            .setup()
            .click(screen.getAllByRole('button', { name: '删掉这段' })[1] as HTMLElement);
        expect(textboxes().map((t) => t.value)).toEqual(['乙', '丙']);
    });

    it('全是文字时「使用这些段」折叠回输入框草稿', async () => {
        const user = userEvent.setup();
        const onApply = renderBuilder({ text: '你好', mentions: [], rich: [] });
        await screen.findByRole('dialog', { name: '消息构建器' });
        fireEvent.change(screen.getByLabelText('文字内容'), { target: { value: '改过的' } });
        await user.click(screen.getByRole('button', { name: '使用这些段' }));
        expect(onApply).toHaveBeenCalledWith({ text: '改过的', mentions: [], rich: [] });
    });

    it('@ 段折叠回输入框时复用旧名字', async () => {
        const user = userEvent.setup();
        const onApply = renderBuilder({
            text: '@阿强 你好',
            mentions: [{ qq: '10003', label: '@阿强' }],
            rich: [],
        });
        await screen.findByRole('dialog', { name: '消息构建器' });
        await user.click(screen.getByRole('button', { name: '使用这些段' }));
        expect(onApply).toHaveBeenCalledWith({
            text: '@阿强 你好',
            mentions: [{ qq: '10003', label: '@阿强' }],
            rich: [],
        });
    });

    it('加图片段：写回时整份留在构建器段里，输入框清空', async () => {
        const user = userEvent.setup();
        const onApply = renderBuilder({ text: '看看这个', mentions: [], rich: [] });
        await screen.findByRole('dialog', { name: '消息构建器' });

        await user.click(screen.getByRole('combobox'));
        await user.click(within(await screen.findByRole('listbox')).getByText('图片'));
        fireEvent.change(screen.getByLabelText('图片地址'), {
            target: { value: 'http://a/1.png' },
        });

        // 预览和消息段 JSON 跟着变
        expect(screen.getByText('看看这个[图片]')).toBeInTheDocument();

        await user.click(screen.getByRole('button', { name: '使用这些段' }));
        expect(onApply).toHaveBeenCalledWith({
            text: '',
            mentions: [],
            rich: [text('看看这个'), image('http://a/1.png')],
        });
    });

    it('坏的 JSON 卡片有提示，但「使用这些段」不拦', async () => {
        const user = userEvent.setup();
        const onApply = renderBuilder({ text: '', mentions: [], rich: [] });
        await screen.findByRole('dialog', { name: '消息构建器' });

        await user.click(screen.getByRole('combobox'));
        await user.click(within(await screen.findByRole('listbox')).getByText('JSON 卡片'));
        fireEvent.change(screen.getByLabelText('卡片 JSON'), { target: { value: '{bad' } });
        expect(screen.getByText('不是合法的 JSON')).toBeInTheDocument();

        await user.click(screen.getByRole('button', { name: '使用这些段' }));
        expect(onApply).toHaveBeenCalledWith({
            text: '',
            mentions: [],
            rich: [{ type: 'json', data: { data: '{bad' } }],
        });
    });

    it('「解析现有内容」放弃对话框里的修改，从草稿重新解析', async () => {
        const user = userEvent.setup();
        renderBuilder({ text: '原始', mentions: [], rich: [] });
        await screen.findByRole('dialog', { name: '消息构建器' });
        fireEvent.change(screen.getByLabelText('文字内容'), { target: { value: '改过的' } });
        await user.click(screen.getByRole('button', { name: '解析现有内容' }));
        expect(screen.getByLabelText('文字内容')).toHaveValue('原始');
    });

    it('输入框上挂着的回复算进预览', async () => {
        renderBuilder({ text: '好', mentions: [], rich: [] }, vi.fn(), 42);
        await screen.findByRole('dialog', { name: '消息构建器' });
        expect(screen.getByLabelText('消息段 JSON')).toHaveTextContent('"type": "reply"');
        expect(screen.getByLabelText('消息段 JSON')).toHaveTextContent('"id": "42"');
    });
});
