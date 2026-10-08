// Bot 卡片拖拽排序的交互层：指针拖拽（手柄 0ms 立即启动 / 卡面 4px 阈值）、
// 跟随浮动 ghost 的 GPU 位移、两两对调的实时预览、FLIP 让位动效。
// 只搬逻辑不碰渲染：ghost 与占位槽的 JSX 留在 BotListGrid，本 hook 交还
// refs 与 displayBots。窗口级 pointer 监听在 effect 里挂卸，rafId 防抖。

import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import gsap from 'gsap';
import { useMotion } from '../../../../hooks/preferences/useMotion';
import type { BotActorSnapshot } from '../../../../core/ipc/types';

interface CardRect {
    id: string;
    left: number;
    right: number;
    top: number;
    bottom: number;
    centerX: number;
    centerY: number;
}

export function useBotCardDrag({
    bots,
    isBatchMode,
    onReorderBots,
}: {
    bots: BotActorSnapshot[];
    isBatchMode: boolean;
    onReorderBots: (sourceBotId: string, targetBotId: string) => void;
}) {
    const m = useMotion();
    const containerRef = useRef<HTMLDivElement>(null);
    const ghostRef = useRef<HTMLDivElement>(null);
    const [draggedId, setDraggedId] = useState<string | null>(null);
    const [hoverTargetId, setHoverTargetId] = useState<string | null>(null);

    const isDraggingRef = useRef(false);
    const startPosRef = useRef<{ x: number; y: number; id: string } | null>(null);
    const cardRectsRef = useRef<CardRect[]>([]);
    const rafIdRef = useRef<number | null>(null);
    const lastHoverTargetRef = useRef<string | null>(null);

    const ghostTiltDeg = !m.enabled
        ? 0
        : m.level === 'rich'
          ? 3.5
          : m.level === 'standard'
            ? 2.0
            : 0.8;

    const ghostScale = !m.enabled
        ? 1.0
        : m.level === 'rich'
          ? 1.06
          : m.level === 'standard'
            ? 1.03
            : 1.01;

    const measureCards = useCallback(() => {
        if (!containerRef.current) return [];
        const elements = containerRef.current.querySelectorAll<HTMLElement>('[data-bot-card-id]');
        const rects: CardRect[] = [];
        elements.forEach((el) => {
            const id = el.getAttribute('data-bot-card-id');
            if (id) {
                const r = el.getBoundingClientRect();
                rects.push({
                    id,
                    left: r.left,
                    right: r.right,
                    top: r.top,
                    bottom: r.bottom,
                    centerX: r.left + r.width / 2,
                    centerY: r.top + r.height / 2,
                });
            }
        });
        return rects;
    }, []);

    const handlePointerDown = (e: React.PointerEvent, botId: string) => {
        if (isBatchMode || e.button !== 0) return;
        const target = e.target as HTMLElement;
        const isHandle = !!target.closest('[data-drag-handle]');
        if (
            !isHandle &&
            target.closest('button, input, [role="button"], a, select, [tabindex], [data-no-drag]')
        ) {
            return;
        }

        const startX = e.clientX;
        const startY = e.clientY;
        startPosRef.current = { x: startX, y: startY, id: botId };

        if (isHandle) {
            // 点击手柄：0 毫秒立即启动拖拽
            isDraggingRef.current = true;
            cardRectsRef.current = measureCards();
            lastHoverTargetRef.current = botId;
            setDraggedId(botId);
            setHoverTargetId(botId);
            if (ghostRef.current) {
                ghostRef.current.style.display = 'flex';
                ghostRef.current.style.transform = `translate3d(${startX + 14}px, ${startY + 14}px, 0) rotate(${ghostTiltDeg}deg) scale(${ghostScale})`;
            }
        }
    };

    useEffect(() => {
        const onPointerMove = (e: PointerEvent) => {
            const start = startPosRef.current;
            if (!start) return;

            if (!isDraggingRef.current) {
                const dist = Math.hypot(e.clientX - start.x, e.clientY - start.y);
                if (dist > 4) {
                    isDraggingRef.current = true;
                    cardRectsRef.current = measureCards();
                    lastHoverTargetRef.current = start.id;
                    setDraggedId(start.id);
                    setHoverTargetId(start.id);
                    if (ghostRef.current) {
                        ghostRef.current.style.display = 'flex';
                    }
                } else {
                    return;
                }
            }

            // 零 React 开销：GPU 直接位移跟随 + 动态姿态微倾角
            if (rafIdRef.current) cancelAnimationFrame(rafIdRef.current);
            rafIdRef.current = requestAnimationFrame(() => {
                if (ghostRef.current) {
                    ghostRef.current.style.transform = `translate3d(${e.clientX + 14}px, ${e.clientY + 14}px, 0) rotate(${ghostTiltDeg}deg) scale(${ghostScale})`;
                }
            });

            // 智能边界盒命中判定（解决对角线穿行时相邻卡片反复震荡乱跳）：
            // 只有当光标明确进入某个卡片区域时才切换插槽，缝隙过渡时平稳保持。
            let targetId = lastHoverTargetRef.current || start.id;
            for (const rect of cardRectsRef.current) {
                if (
                    e.clientX >= rect.left &&
                    e.clientX <= rect.right &&
                    e.clientY >= rect.top &&
                    e.clientY <= rect.bottom
                ) {
                    targetId = rect.id;
                    break;
                }
            }

            if (targetId !== lastHoverTargetRef.current) {
                lastHoverTargetRef.current = targetId;
                setHoverTargetId(targetId);
            }
        };

        const onPointerUp = () => {
            if (rafIdRef.current) {
                cancelAnimationFrame(rafIdRef.current);
                rafIdRef.current = null;
            }

            const currentDragged = startPosRef.current?.id;
            const currentTarget = lastHoverTargetRef.current;

            if (
                isDraggingRef.current &&
                currentDragged &&
                currentTarget &&
                currentDragged !== currentTarget
            ) {
                onReorderBots(currentDragged, currentTarget);
                // 放置成功时播放符合当前动效档位的微回弹反馈
                if (m.enabled && containerRef.current) {
                    const droppedEl = containerRef.current.querySelector<HTMLElement>(
                        `[data-bot-card-id="${currentDragged}"]`,
                    );
                    if (droppedEl) {
                        m.pop(droppedEl, { ease: 'release' });
                    }
                }
            }

            isDraggingRef.current = false;
            startPosRef.current = null;
            lastHoverTargetRef.current = null;
            setDraggedId(null);
            setHoverTargetId(null);
            if (ghostRef.current) {
                ghostRef.current.style.display = 'none';
            }
        };

        window.addEventListener('pointermove', onPointerMove, { passive: true });
        window.addEventListener('pointerup', onPointerUp);
        window.addEventListener('pointercancel', onPointerUp);
        return () => {
            window.removeEventListener('pointermove', onPointerMove);
            window.removeEventListener('pointerup', onPointerUp);
            window.removeEventListener('pointercancel', onPointerUp);
        };
    }, [ghostScale, ghostTiltDeg, m, measureCards, onReorderBots]);

    // 计算拖拽中的实时预览（两两对调模式：对角线移动时仅对调目标卡片，其余卡片保持静止）
    const displayBots = useMemo(() => {
        if (!draggedId || !hoverTargetId || draggedId === hoverTargetId) {
            return bots;
        }
        const srcIdx = bots.findIndex((b) => b.bot_id === draggedId);
        const dstIdx = bots.findIndex((b) => b.bot_id === hoverTargetId);
        if (srcIdx === -1 || dstIdx === -1) return bots;

        const next = [...bots];
        const temp = next[srcIdx];
        next[srcIdx] = next[dstIdx];
        next[dstIdx] = temp;
        return next;
    }, [bots, draggedId, hoverTargetId]);

    // FLIP (First-Last-Invert-Play) 实时滑动让位动效：
    // 用户移动卡片时，其他被挤压/替换的卡片会丝滑滑向新位置，视觉极其直观。
    const prevRectsRef = useRef<Map<string, DOMRect>>(new Map());

    useLayoutEffect(() => {
        if (!containerRef.current) return;
        const elements = containerRef.current.querySelectorAll<HTMLElement>('[data-bot-card-id]');
        const currentRects = new Map<string, DOMRect>();

        elements.forEach((el) => {
            const id = el.getAttribute('data-bot-card-id');
            if (id) {
                currentRects.set(id, el.getBoundingClientRect());
            }
        });

        elements.forEach((el) => {
            const id = el.getAttribute('data-bot-card-id');
            if (!id || id === draggedId) return;

            const prev = prevRectsRef.current.get(id);
            const current = currentRects.get(id);

            if (prev && current) {
                const deltaX = prev.left - current.left;
                const deltaY = prev.top - current.top;

                if (deltaX !== 0 || deltaY !== 0) {
                    if (m.enabled) {
                        gsap.fromTo(
                            el,
                            { x: deltaX, y: deltaY },
                            {
                                x: 0,
                                y: 0,
                                duration: Math.max(0.2, m.duration('fast') * 1.1),
                                ease: 'power2.out',
                                overwrite: 'auto',
                            },
                        );
                    }
                }
            }
        });

        prevRectsRef.current = currentRects;
    }, [displayBots, draggedId, m]);

    return {
        containerRef,
        ghostRef,
        draggedId,
        displayBots,
        handlePointerDown,
    };
}
