// 全局 title 属性事件委托拦截器与现代气泡注入器。
//
// 机制：
// 1. 全局监听 pointerover/pointerout，自动捕获 DOM 中的 `title="..."` 或 `data-tooltip="..."`；
// 2. 临时清空原生 title 并在离开时恢复，彻底屏蔽 Windows/WebView 的黄色丑陋原生气泡；
// 3. 智能计算元素位置与边界（贴顶/贴边自动翻转，支持工具栏相邻元素快速切换 0 延迟）；
// 4. 采用设计系统统一 Token（bg-text text-canvas text-2xs font-medium shadow-popover）。

import React, { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';

interface TooltipState {
    text: string;
    x: number;
    y: number;
    side: 'top' | 'bottom';
    visible: boolean;
}

const DEFAULT_DELAY_MS = 500;
const WARM_WINDOW_MS = 250;

export const GlobalTitleTooltip: React.FC = () => {
    const [state, setState] = useState<TooltipState | null>(null);
    const targetRef = useRef<HTMLElement | null>(null);
    const showTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
    const lastVisibleHideTimeRef = useRef<number>(0);
    const isVisibleRef = useRef<boolean>(false);
    const tooltipElementRef = useRef<HTMLDivElement | null>(null);

    useEffect(() => {
        const findTarget = (start: Element | null): HTMLElement | null => {
            if (!start || !(start instanceof Element)) return null;

            // 忽略显式接入 Radix Tooltip 的组件
            if (start.closest('[data-radix-tooltip-trigger]')) return null;

            const el = start.closest('[title], [data-tooltip], [data-native-title]') as HTMLElement | null;
            if (!el || el === document.body || el === document.documentElement) return null;
            if (el.hasAttribute('data-no-tooltip')) return null;

            return el;
        };

        const updatePosition = (el: HTMLElement, text: string) => {
            const rect = el.getBoundingClientRect();
            if (rect.width === 0 && rect.height === 0) return;

            const gap = 6;
            // 距视口顶部过近（如右上角窗口控制按钮）时翻转到下方
            const isNearTop = rect.top < 40;
            const side: 'top' | 'bottom' = isNearTop ? 'bottom' : 'top';

            const centerX = rect.left + rect.width / 2;
            const targetY = isNearTop ? rect.bottom + gap : rect.top - gap;

            isVisibleRef.current = true;
            setState({
                text,
                x: centerX,
                y: targetY,
                side,
                visible: true,
            });
        };

        const restoreTitle = (el: HTMLElement | null) => {
            if (el && el.dataset.nativeTitle !== undefined) {
                el.setAttribute('title', el.dataset.nativeTitle);
                delete el.dataset.nativeTitle;
            }
        };

        const hideTooltip = () => {
            if (showTimerRef.current) {
                clearTimeout(showTimerRef.current);
                showTimerRef.current = null;
            }
            if (targetRef.current) {
                restoreTitle(targetRef.current);
                targetRef.current = null;
            }
            if (isVisibleRef.current) {
                lastVisibleHideTimeRef.current = Date.now();
                isVisibleRef.current = false;
            }
            setState((prev) => (prev ? { ...prev, visible: false } : null));
        };

        const handlePointerOver = (e: PointerEvent) => {
            if (e.pointerType === 'touch') return;

            const target = findTarget(e.target as Element | null);
            if (!target) {
                hideTooltip();
                return;
            }

            // 临时将 title 移至 dataset.nativeTitle，并置空 title 避免浏览器弹出黄底原生方块
            if (target.hasAttribute('title')) {
                const titleVal = target.getAttribute('title');
                if (titleVal && titleVal.trim().length > 0) {
                    target.dataset.nativeTitle = titleVal;
                    target.setAttribute('title', '');
                }
            }

            const text = target.dataset.nativeTitle || target.dataset.tooltip;
            if (!text || text.trim().length === 0) {
                hideTooltip();
                return;
            }

            if (targetRef.current === target) return;

            // 恢复上一个目标的 title
            if (targetRef.current && targetRef.current !== target) {
                restoreTitle(targetRef.current);
            }

            targetRef.current = target;
            if (showTimerRef.current) clearTimeout(showTimerRef.current);

            // 只有当上一个 tooltip 已经真实显形过且在短时间内移到相邻项，才触发快速连续响应；否则必须等待标准的 500ms 悬停延迟
            const isWarm =
                lastVisibleHideTimeRef.current > 0 &&
                Date.now() - lastVisibleHideTimeRef.current < WARM_WINDOW_MS;
            const delay = isWarm ? 60 : DEFAULT_DELAY_MS;

            showTimerRef.current = setTimeout(() => {
                if (targetRef.current === target) {
                    updatePosition(target, text);
                }
            }, delay);
        };

        const handlePointerOut = (e: PointerEvent) => {
            const nextTarget = findTarget(e.relatedTarget as Element | null);
            if (!nextTarget || nextTarget !== targetRef.current) {
                hideTooltip();
            }
        };

        const handleDismiss = () => {
            hideTooltip();
        };

        document.addEventListener('pointerover', handlePointerOver, { passive: true });
        document.addEventListener('pointerout', handlePointerOut, { passive: true });
        document.addEventListener('pointerdown', handleDismiss, { passive: true });
        document.addEventListener('scroll', handleDismiss, { capture: true, passive: true });
        window.addEventListener('blur', handleDismiss);

        return () => {
            document.removeEventListener('pointerover', handlePointerOver);
            document.removeEventListener('pointerout', handlePointerOut);
            document.removeEventListener('pointerdown', handleDismiss);
            document.removeEventListener('scroll', handleDismiss, { capture: true });
            window.removeEventListener('blur', handleDismiss);
            if (showTimerRef.current) clearTimeout(showTimerRef.current);
            if (targetRef.current) restoreTitle(targetRef.current);
        };
    }, []);

    // 在 DOM 绘制前精确测量尺寸，进行边缘安全距离限制与位置平移，绝不粗暴换行
    useLayoutEffect(() => {
        const el = tooltipElementRef.current;
        if (!el || !state?.visible) return;

        const width = el.offsetWidth;
        const height = el.offsetHeight;
        const padding = 10;

        // 计算水平居中并进行屏幕左右防溢出 clamp，完全在视口内平移，保持单行
        let left = state.x - width / 2;
        if (left < padding) {
            left = padding;
        } else if (left + width > window.innerWidth - padding) {
            left = Math.max(padding, window.innerWidth - padding - width);
        }

        // 计算垂直坐标（top 模式向上展开，bottom 模式向下展开）
        let top = state.side === 'top' ? state.y - height : state.y;
        if (top < padding) {
            top = padding;
        } else if (top + height > window.innerHeight - padding) {
            top = window.innerHeight - padding - height;
        }

        el.style.left = `${Math.round(left)}px`;
        el.style.top = `${Math.round(top)}px`;
    }, [state]);

    if (!state || !state.text) return null;

    return createPortal(
        <div
            ref={tooltipElementRef}
            role="tooltip"
            aria-hidden="true"
            style={{
                position: 'fixed',
                left: '-9999px',
                top: '-9999px',
                width: 'max-content',
                maxWidth: 'calc(100vw - 20px)',
                pointerEvents: 'none',
                zIndex: 9999,
                willChange: 'transform, opacity',
            }}
            className={`select-none whitespace-nowrap rounded-sm bg-text/95 px-2.5 py-1 text-2xs font-medium text-canvas shadow-popover backdrop-blur-xs transition-opacity duration-150 ease-out ${state.visible ? 'opacity-100' : 'opacity-0'
                }`}
        >
            {state.text}
        </div>,
        document.body,
    );
};


