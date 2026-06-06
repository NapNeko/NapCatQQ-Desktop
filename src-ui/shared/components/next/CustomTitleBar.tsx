// 自定义窗口标题栏（next）。
//
// 视觉决策（不像 Win11 chrome，融入画布）：
//   - 完全透明背景，无边框，无 backdrop-blur
//   - 不画 logo / 应用名，让 sidebar 顶部的 NapCat 卡片自然延伸到顶
//   - 仅 3 个窗口控制键浮在右上，hover 时背景才显形
//   - 整条作为 drag region（drag region 自带双击最大化）
//
// 分层（frontend-layering §2.5）：禁止 import @tauri-apps/*，
// 通过 useWindowControls hook → windowControlService 调用窗口动作。

import React from 'react';
import { Maximize2, Minus, Square, X as CloseIcon } from 'lucide-react';
import { cn } from '../../utils/cn';
import { useWindowControls } from '../../../hooks/desktop/useWindowControls';
import { MotionIcon } from '../../ui/motion/MotionIcon';

interface CustomTitleBarProps {
  className?: string;
}

export const CustomTitleBar: React.FC<CustomTitleBarProps> = ({ className }) => {
  const { isMaximized, minimize, toggleMaximize, close } = useWindowControls();

  return (
    <header
      className={cn(
        'relative z-30 flex h-12 shrink-0 select-none items-center',
        'bg-transparent',
        className,
      )}
    >
      {/* 整条都是 drag region；右侧三个按钮通过 stopPropagation 避免拖动 */}
      <div className="h-full flex-1" data-tauri-drag-region />

      <div className="flex h-full shrink-0 items-stretch">
        <WindowButton onClick={minimize} aria-label="最小化">
          <MotionIcon icon={Minus} motion="none" hoverAccent playEnter={false} size={12} strokeWidth={1.75} />
        </WindowButton>
        <WindowButton
          onClick={toggleMaximize}
          aria-label={isMaximized ? '还原' : '最大化'}
        >
          {isMaximized ? (
            <MotionIcon icon={Square} motion="none" hoverAccent playEnter={false} enterKey="max" size={10} strokeWidth={1.75} />
          ) : (
            <MotionIcon icon={Maximize2} motion="none" hoverAccent playEnter={false} enterKey="restore" size={10} strokeWidth={1.75} />
          )}
        </WindowButton>
        <WindowButton onClick={close} aria-label="关闭" tone="danger">
          <MotionIcon icon={CloseIcon} motion="none" hoverAccent playEnter={false} size={12} strokeWidth={1.75} />
        </WindowButton>
      </div>
    </header>
  );
};

interface WindowButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  tone?: 'default' | 'danger';
}

const WindowButton: React.FC<WindowButtonProps> = ({
  className,
  tone = 'default',
  children,
  ...props
}) => (
  <button
    type="button"
    className={cn(
      'inline-flex h-full w-10 items-center justify-center text-text-tertiary transition-colors',
      'hover:text-text',
      tone === 'default' && 'hover:bg-text/8',
      tone === 'danger' && 'hover:bg-danger hover:text-white',
      'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-brand',
      className,
    )}
    {...props}
  >
    {children}
  </button>
);

export default CustomTitleBar;
