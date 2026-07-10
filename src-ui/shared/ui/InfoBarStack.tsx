// 多 InfoBar 堆叠容器。GSAP 版。
//
// 退场处理:items 减少时,被移除的 id 仍保留在本地 displayedIds 中(visible=false),
// 让 GsapPresence 跑完 exit 才真 unmount;真 unmount 后 onExited 回调里把 id
// 从 displayedIds 移掉。新增 id 也走类似流程。
//
// items 改顺序的场景不处理(目前业务都是只 push 不重排)。

import { createPortal } from 'react-dom';
import gsap from 'gsap';
import { useEffect, useState, type ReactNode } from 'react';
import { InfoBar, type InfoBarProps } from './InfoBar';
import { GsapPresence, type EnterFn, type ExitFn } from './motion/GsapPresence';
import { cn } from '../utils/cn';

export interface InfoBarStackItem extends Omit<InfoBarProps, 'onDismiss'> {
    id: string;
    /** 用户点关闭时由 globalInfoBarStore.dismiss 调用，不传给 InfoBar DOM。 */
    onUserDismiss?: () => void;
}

interface InfoBarStackProps {
    items: InfoBarStackItem[];
    onDismiss: (id: string) => void;
    onAutoDismiss?: (id: string) => void;
    className?: string;
    portal?: boolean;
    children?: ReactNode;
}

/// enter 工厂按 tone 分:danger/warning 进场更急(fast 替代 base) + 落位 shake;
/// success/info 走标准 release。生成函数返回 EnterFn 闭包。
function makeEnter(tone: InfoBarStackItem['tone']): EnterFn {
    return (el, env) => {
        const urgent = tone === 'danger' || tone === 'warning';
        const tl = gsap.timeline();
        tl.fromTo(
            el,
            { autoAlpha: 0, x: 16, scale: 0.985 },
            {
                autoAlpha: 1,
                x: 0,
                scale: 1,
                duration: urgent ? env.duration('fast') : env.duration('base'),
                ease: env.ease.release,
            },
        );
        // danger 落位 shake;warning/success 不 shake;rich 档 success 给一次轻 pop。
        if (tone === 'danger' && env.preset.feel.shakeAmplitude > 0) {
            tl.add(() => env.shake(el));
        } else if (tone === 'success' && env.preset.feel.popPeak > 1) {
            tl.add(() => env.pop(el, { peak: 1 + (env.preset.feel.popPeak - 1) * 0.5 }));
        }
        return tl;
    };
}

const exit: ExitFn = (el, env) =>
    gsap.to(el, {
        autoAlpha: 0,
        x: 12,
        duration: env.duration('fast'),
        ease: env.ease.exit,
    });

interface DisplayedItem extends InfoBarStackItem {
    /// false 表示已被父级移除,等 exit 跑完再真 unmount。
    visible: boolean;
}

export function InfoBarStack({
    items,
    onDismiss,
    onAutoDismiss,
    className,
    portal = true,
    children,
}: InfoBarStackProps) {
    const [displayed, setDisplayed] = useState<DisplayedItem[]>(() =>
        items.map((it) => ({ ...it, visible: true })),
    );
    // items 任意变更触发 displayed reconciliation:
    //   - 新 id → 追加 visible=true
    //   - 已有 id → 更新数据(title/content 可能变了)visible=true
    //   - 不在 items 但在 displayed → 标 visible=false 跑 exit
    useEffect(() => {
        const incomingIds = new Set(items.map((i) => i.id));
        setDisplayed((prev) => {
            const next: DisplayedItem[] = [];
            // 先按 prev 顺序保留(被移除的也留着,visible=false 跑 exit)
            for (const p of prev) {
                const incoming = items.find((i) => i.id === p.id);
                if (incoming) {
                    next.push({ ...incoming, visible: true });
                } else if (p.visible) {
                    // 刚被移除,标 visible=false 跑 exit
                    next.push({ ...p, visible: false });
                } else {
                    // 已经 visible=false,沿用(不应该到这里,因为 exit 完会清掉)
                    next.push(p);
                }
            }
            // 新 id 追加到末尾
            for (const i of items) {
                if (!prev.some((p) => p.id === i.id)) {
                    next.push({ ...i, visible: true });
                }
            }
            // 防一种边缘:incoming 里有 prev 中已经 visible=false 的 id(被快速 push-pop-push)
            // 把它们 force 回 true。
            for (let k = 0; k < next.length; k++) {
                if (incomingIds.has(next[k].id)) {
                    next[k] = { ...next[k], visible: true };
                }
            }
            return next;
        });
    }, [items]);

    if (displayed.length === 0 && !children) return null;

    const node = (
        <div
            className={cn(
                'pointer-events-none fixed right-6 top-[calc(var(--titlebar-height)+1.5rem)] z-50 flex w-[min(420px,calc(100vw-3rem))] flex-col gap-4',
                className,
            )}
        >
            {displayed.map((item) => {
                const { onUserDismiss: _omit, ...barProps } = item;
                return (
                <GsapPresence
                    key={item.id}
                    visible={item.visible}
                    onEnter={makeEnter(item.tone)}
                    onExit={exit}
                    onExited={() => {
                        setDisplayed((prev) => prev.filter((p) => p.id !== item.id));
                    }}
                >
                    <InfoBarRow
                        {...barProps}
                        onDismiss={() => onDismiss(item.id)}
                        onAutoDismiss={() => (onAutoDismiss ?? onDismiss)(item.id)}
                    />
                </GsapPresence>
                );
            })}
            {children}
        </div>
    );

    if (!portal || typeof document === 'undefined') return node;
    return createPortal(node, document.body);
}

/// 内层 forwardRef wrapper:把 ref 直接转给 InfoBar(InfoBar 已 forwardRef)。
import { forwardRef } from 'react';
const InfoBarRow = forwardRef<HTMLDivElement, InfoBarProps>((props, ref) => (
    <InfoBar ref={ref} {...props} />
));
InfoBarRow.displayName = 'InfoBarRow';
