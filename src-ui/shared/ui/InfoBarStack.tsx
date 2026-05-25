// 多 InfoBar 堆叠容器。负责 portal 出口 + 排版 + 退出动画时机。
//
// 使用模式（受控）：父级 hook 维护 banner 列表，每条 banner 唯一 id；
// stack 渲染 InfoBar，用户点关闭时调 onDismiss(id) 回到 hook 删条目。
//
// 位置：默认 fixed top-right 贴 TitleBar 下方（top: 64px 让出 48px 窗口控件高度
// + 一点 buffer），堆叠方向自上而下。可通过 className 覆盖。
//
// 为什么不用 React portal createPortal：当前 AppShell 已经 z-30 标题栏 +
// z-10 主区，fixed 元素 z-index 50 已经够用，没必要专门引入 portal 节点。

import { createPortal } from 'react-dom';
import type { ReactNode } from 'react';
import { InfoBar, type InfoBarProps } from './InfoBar';
import { cn } from '../utils/cn';

export interface InfoBarStackItem extends Omit<InfoBarProps, 'onDismiss'> {
    id: string;
}

interface InfoBarStackProps {
    items: InfoBarStackItem[];
    onDismiss: (id: string) => void;
    /** 容器追加 className。默认 fixed top-right 贴 TitleBar 下方。 */
    className?: string;
    /** 是否走 document.body portal。默认 true，避免被父级 overflow:hidden 切。 */
    portal?: boolean;
    /** 可选自定义内容附加在 InfoBar 内（极少需要）。 */
    children?: ReactNode;
}

export function InfoBarStack({
    items,
    onDismiss,
    className,
    portal = true,
    children,
}: InfoBarStackProps) {
    if (items.length === 0 && !children) return null;

    const node = (
        <div
            className={cn(
                // fixed 顶层堆叠，不参与父级布局
                'pointer-events-none fixed right-6 top-[64px] z-50 flex w-[min(420px,calc(100vw-3rem))] flex-col gap-2',
                className,
            )}
        >
            {items.map((item) => (
                <InfoBar
                    key={item.id}
                    {...item}
                    onDismiss={() => onDismiss(item.id)}
                />
            ))}
            {children}
        </div>
    );

    if (!portal || typeof document === 'undefined') return node;
    return createPortal(node, document.body);
}
