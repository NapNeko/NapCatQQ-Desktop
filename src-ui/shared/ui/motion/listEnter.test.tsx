import { render, renderHook } from '@testing-library/react';
import { useGSAP } from '@gsap/react';
import gsap from 'gsap';
import { useRef } from 'react';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { preferencesStore } from '../../../core/domain/settings/preferencesStore';
import { useMotion } from '../../../hooks/preferences/useMotion';
import { ListItem } from './ListItem';
import { animateListChildrenEnter } from './listEnter';

// 时间由测试手动推进，不跟 ticker 走，每一步停在哪一刻都是确定的
beforeEach(() => {
    gsap.ticker.remove(gsap.updateRoot);
});
afterEach(() => {
    gsap.ticker.add(gsap.updateRoot);
    preferencesStore.reset();
});

// 按 60 帧一格一格推：一次跳一大段时，同一帧里先收尾的进场和后渲染的悬停谁盖谁，和真实播放不一样
const FRAME = 1 / 60;
const advance = (sec: number) => {
    for (let left = sec; left > 1e-9; left -= FRAME) {
        gsap.updateRoot(gsap.globalTimeline.time() + Math.min(FRAME, left));
    }
};

/** 和应用端列表同一种搭法：列表容器跑进场，每张卡是带悬停上浮的 ListItem */
function Cards({ n }: { n: number }) {
    const m = useMotion();
    const ref = useRef<HTMLDivElement>(null);
    useGSAP(
        () => {
            if (ref.current) animateListChildrenEnter(ref.current, n, m);
        },
        { scope: ref, dependencies: [n] },
    );
    return (
        <div ref={ref}>
            {Array.from({ length: n }, (_, i) => (
                <ListItem key={i} hoverable data-testid={`card-${i}`}>
                    卡片 {i}
                </ListItem>
            ))}
        </div>
    );
}

describe('列表进场遇上悬停', () => {
    it('进场途中指针移上一张卡：它照常淡入，整组播完后上浮还在', () => {
        const m = renderHook(() => useMotion()).result.current;
        const dur = m.duration('base');
        const gap = m.stagger();
        const lift = m.preset.feel.cardLift;
        // 默认档位有错峰、有上浮，这个场景才成立
        expect(gap).toBeGreaterThan(0);
        expect(lift).toBeGreaterThan(0);

        const { getByTestId } = render(<Cards n={3} />);
        const card = getByTestId('card-1');

        advance(gap + dur * 0.3);
        const before = Number(card.style.opacity);
        expect(before).toBeGreaterThan(0);
        expect(before).toBeLessThan(1);

        card.dispatchEvent(new MouseEvent('mouseenter'));
        advance(dur * 0.3);
        // 以前悬停把这张卡的进场整个掐掉，透明度停在这一刻
        expect(Number(card.style.opacity)).toBeGreaterThan(before);

        advance(gap * 2 + dur + 1);
        for (const i of [0, 1, 2]) {
            expect(getByTestId(`card-${i}`).style.opacity).toBe('1');
            expect(getByTestId(`card-${i}`).style.visibility).not.toBe('hidden');
        }
        // 指针还在卡上：进场收尾不能把悬停的上浮拉回 0
        expect(gsap.getProperty(card, 'y')).toBe(-lift);
    });
});
