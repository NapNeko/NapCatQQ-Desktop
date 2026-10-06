// useSlideAfterShift：左栏收起 / 展开时中栏内容补一段滑动。
// 重点是「快速收起→立刻展开」不跳：记录时量的是内容节点带 transform 的视觉位置，
// 新一段动画出发前先 cancel 上一段，再从内容此刻的布局位置量起滑。

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, render } from '@testing-library/react';
import { useRef } from 'react';
import { preferencesStore } from '../../hooks/preferences/preferencesStore';
import { useSlideAfterShift } from './ColumnFrame';

// jsdom 的量不出来真实布局，这里按「内容节点」和「section」分开给：section 的位置故意给得
// 很偏，谁错用了 section 的 rect 会一眼看出来（见 F9-M1：收起滑到一半展开，内容往回跳）
let contentLeft = 48;
const SECTION_LEFT = 900;

function rect(left: number): DOMRect {
    return {
        left,
        right: left + 200,
        top: 0,
        bottom: 10,
        width: 200,
        height: 10,
        x: left,
        y: 0,
        toJSON: () => ({}),
    } as DOMRect;
}

interface FakeAnimation {
    cancel: ReturnType<typeof vi.fn>;
    keyframes: Keyframe[];
}
const anims: FakeAnimation[] = [];

// 挂在 hook 里把 capture 函数拿出来
let captureFn: (() => void) | null = null;

function Harness({ shift }: { shift: unknown }) {
    const ref = useRef<HTMLElement>(null);
    captureFn = useSlideAfterShift(ref, shift);
    return (
        <section ref={ref}>
            <div data-column-body="">
                <div className="content">内容</div>
            </div>
        </section>
    );
}

beforeEach(() => {
    preferencesStore.setMotionEnabled(true);
    contentLeft = 48;
    anims.length = 0;
    vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockImplementation(function (
        this: HTMLElement,
    ) {
        return rect(this.classList.contains('content') ? contentLeft : SECTION_LEFT);
    });
    // jsdom 的 Element.animate 由测试替换成只管 cancel 记录的假动画
    (Element.prototype as { animate?: unknown }).animate = function (
        this: Element,
        keyframes: Keyframe[],
    ) {
        const a: FakeAnimation = { cancel: vi.fn(), keyframes };
        anims.push(a);
        return a;
    };
});

afterEach(() => {
    cleanup();
    preferencesStore.reset();
    captureFn = null;
});

describe('useSlideAfterShift', () => {
    it('布局换了之后，内容从原来的位置滑到新位置', () => {
        const { rerender } = render(<Harness shift={false} />);
        act(() => captureFn?.());
        // 布局瞬间换完：内容现在画在 244
        contentLeft = 244;
        rerender(<Harness shift={true} />);
        expect(anims).toHaveLength(1);
        expect(anims[0].keyframes[0]).toEqual({ transform: 'translateX(-196px)' });
        expect(anims[0].keyframes[1]).toEqual({ transform: 'none' });
    });

    it('滑到一半就反向切换：量内容带 transform 的位置接着滑，不往回跳', () => {
        const { rerender } = render(<Harness shift={false} />);
        // 收起：内容从 48 滑到 244
        act(() => captureFn?.());
        contentLeft = 244;
        rerender(<Harness shift={true} />);
        expect(anims).toHaveLength(1);

        // 滑到一半（视觉位置 146）立刻展开：getBoundingClientRect 量到的是含 transform 的 146
        contentLeft = 146;
        act(() => captureFn?.());
        // 展开后布局位置回到 48（cancel 掉上一段之后内容回到布局位置）
        contentLeft = 48;
        rerender(<Harness shift={false} />);

        expect(anims).toHaveLength(2);
        // 上一段必须先停掉，否则量出来的起点和内容显示的位置对不上
        expect(anims[0].cancel).toHaveBeenCalled();
        // 从视觉位置 146 接着滑到位移 0（dx = 146 - 48）；用 section 的 rect 会算出 900 - 48
        expect(anims[1].keyframes[0]).toEqual({ transform: 'translateX(98px)' });
    });

    it('关了动画时什么都不播', () => {
        preferencesStore.setMotionEnabled(false);
        const { rerender } = render(<Harness shift={false} />);
        act(() => captureFn?.());
        contentLeft = 244;
        rerender(<Harness shift={true} />);
        expect(anims).toHaveLength(0);
    });
});
