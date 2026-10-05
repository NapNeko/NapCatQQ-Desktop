import { beforeEach, describe, expect, it } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { GlobalTitleTooltip } from './GlobalTitleTooltip';

// 这一层是「所有小按钮都有气泡」的兜底：产品里图标按钮普遍只写了 aria-label
// （给读屏用），而只有 aria-label 是没有视觉气泡的。逐个补 title 容易漏，
// 所以在全局机制里回落一次。下面把这些边界钉住。

const hover = (el: Element) => fireEvent.pointerOver(el, { bubbles: true });

// jsdom 里所有元素尺寸都是 0，而组件在「0×0 就不定位」上直接返回——不桩这个，
// 正例永远等不到气泡（负例则会「因为压根没渲染」而假通过）。
beforeEach(() => {
    HTMLElement.prototype.getBoundingClientRect = function () {
        return {
            x: 10,
            y: 10,
            top: 10,
            left: 10,
            right: 110,
            bottom: 34,
            width: 100,
            height: 24,
            toJSON: () => ({}),
        } as DOMRect;
    };
});

describe('GlobalTitleTooltip', () => {
    it('title 属性照旧出气泡', async () => {
        render(
            <>
                <GlobalTitleTooltip />
                <button title="启动实例">x</button>
            </>,
        );
        hover(screen.getByTitle('启动实例'));
        // 气泡本身带 aria-hidden（纯视觉提示），所以按文本查而不是按 role
        await waitFor(() => expect(screen.getByText('启动实例')).toBeTruthy());
    });

    it('只有 aria-label 的图标按钮也出气泡（兜底）', async () => {
        render(
            <>
                <GlobalTitleTooltip />
                <button type="button" aria-label="重新探测" />
            </>,
        );
        hover(screen.getByLabelText('重新探测'));
        await waitFor(() => expect(screen.getByText('重新探测')).toBeTruthy());
    });

    it('有可见文字的按钮不给气泡——哪怕写了 aria-label', async () => {
        render(
            <>
                <GlobalTitleTooltip />
                <button type="button" aria-label="保存">
                    保存
                </button>
            </>,
        );
        hover(screen.getByLabelText('保存'));
        // 等过一个完整延迟都还没有，才算「确实不给」
        await new Promise((r) => setTimeout(r, 700));
        expect(document.querySelector('[role="tooltip"]')).toBeNull();
    });

    it('容器（nav / 输入框）上的 aria-label 不算提示', async () => {
        render(
            <>
                <GlobalTitleTooltip />
                <nav aria-label="分页" />
                <input aria-label="安装目录" />
            </>,
        );
        hover(screen.getByLabelText('分页'));
        hover(screen.getByLabelText('安装目录'));
        await new Promise((r) => setTimeout(r, 700));
        expect(document.querySelector('[role="tooltip"]')).toBeNull();
    });

    it('data-no-tooltip 区域整体不提示', async () => {
        render(
            <>
                <GlobalTitleTooltip />
                <div data-no-tooltip>
                    <button type="button" aria-label="删除" />
                </div>
            </>,
        );
        hover(screen.getByLabelText('删除'));
        await new Promise((r) => setTimeout(r, 700));
        expect(document.querySelector('[role="tooltip"]')).toBeNull();
    });
});
