// 左侧导航栏(next)。第二轮:active 强调线从"每行各自渲染"改 FLIP 单条滑动。
//
// 视觉:active 切换时左侧 2px brand 渐变线从旧 nav 行平滑滑到新行,跟踪 y 位置 +
// 高度。standard/rich 档启用 GSAP 滑动,elegant 档退化为静态显示。
//
// hover/active 配色保持原样;折叠态展开按钮的 hover swap 也保留 CSS transition。

import React, { useLayoutEffect, useRef } from 'react';
import {
    Bot,
    ChevronsLeft,
    ChevronsRight,
    Container,
    LayoutDashboard,
    ListTodo,
    Loader2,
    type LucideIcon,
    Package,
    Server,
    Settings,
} from 'lucide-react';
import gsap from 'gsap';
import { cn } from '../../utils/cn';
import { MotionIcon, NAV_ROUTE_MOTION } from '../../ui/motion';
import { useMotion } from '../../../hooks/preferences/useMotion';
import logoSidebar from '../../../assets/logo-32.png';
import logoSidebarCollapsed from '../../../assets/logo-48.png';

export type AppRoute =
    | 'overview'
    | 'bots'
    | 'components'
    | 'docker'
    | 'remote'
    | 'tasks'
    | 'settings';

interface SidebarProps {
    active: AppRoute;
    onChange: (route: AppRoute) => void;
    collapsed: boolean;
    onToggleCollapse: () => void;
    /// 是否显示 Docker 项。
    showDocker?: boolean;
    taskQueueActiveCount?: number;
}

interface NavItem {
    id: AppRoute;
    label: string;
    icon: LucideIcon;
}

const MAIN_NAV: NavItem[] = [
    { id: 'overview', label: '概览', icon: LayoutDashboard },
    { id: 'bots', label: '机器人', icon: Bot },
    { id: 'components', label: '组件', icon: Package },
    { id: 'docker', label: '容器', icon: Container },
    { id: 'remote', label: '远端', icon: Server },
];

const SETTINGS_NAV: NavItem = {
    id: 'settings',
    label: '设置',
    icon: Settings,
};

const TASKS_NAV: NavItem = {
    id: 'tasks',
    label: '任务',
    icon: ListTodo,
};

const LOGO_IMG_CLASS =
    'select-none object-contain [image-rendering:-webkit-optimize-contrast]';

export const Sidebar: React.FC<SidebarProps> = ({
    active,
    onChange,
    collapsed,
    onToggleCollapse,
    showDocker = true,
    taskQueueActiveCount = 0,
}) => {
    const mainNavItems = showDocker
        ? MAIN_NAV
        : MAIN_NAV.filter((item) => item.id !== 'docker');

    const m = useMotion();
    const navRef = useRef<HTMLElement | null>(null);
    const indicatorRef = useRef<HTMLSpanElement | null>(null);

    // active 变化时把 indicator FLIP 滑到新的 nav 行。读 DOM 找 aria-current="page"
    // 元素,算 top + height,GSAP tween indicator 的 y / height。
    useLayoutEffect(() => {
        const nav = navRef.current;
        const indicator = indicatorRef.current;
        if (!nav || !indicator) return;
        const activeBtn = nav.querySelector<HTMLElement>(
            'button[aria-current="page"]',
        );
        if (!activeBtn) {
            gsap.set(indicator, { autoAlpha: 0 });
            return;
        }
        const navRect = nav.getBoundingClientRect();
        const btnRect = activeBtn.getBoundingClientRect();
        const top = btnRect.top - navRect.top + 6;
        const height = btnRect.height - 12;
        if (!m.enabled || !m.preset.feel.cardLift) {
            gsap.set(indicator, { autoAlpha: 1, y: top, height });
            return;
        }
        gsap.to(indicator, {
            autoAlpha: 1,
            y: top,
            height,
            duration: m.duration('base'),
            ease: m.ease.hover,
        });
    }, [active, collapsed, showDocker, m]);

    return (
        <aside
            className={cn(
                'relative z-20 flex shrink-0 flex-col bg-sidebar',
                'transition-[width] duration-200 ease-out',
                collapsed ? 'w-14' : 'w-52',
            )}
        >
            <div
                className={cn(
                    'flex h-12 shrink-0 items-center',
                    collapsed ? 'justify-center px-0' : 'gap-2.5 px-3',
                )}
            >
                {collapsed ? (
                    <button
                        type="button"
                        onClick={onToggleCollapse}
                        aria-label="展开侧栏"
                        title="展开侧栏"
                        className={cn(
                            'group relative inline-flex h-9 w-9 items-center justify-center rounded-sm',
                            'transition-colors hover:bg-text/5',
                            'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-brand',
                        )}
                    >
                        <img
                            src={logoSidebarCollapsed}
                            alt="NapCatQQ-Desktop logo"
                            width={28}
                            height={28}
                            className={cn('h-7 w-7 transition-opacity group-hover:opacity-0', LOGO_IMG_CLASS)}
                            draggable={false}
                        />
                        <MotionIcon
                            icon={ChevronsRight}
                            motion="none"
                            hoverAccent
                            size={16}
                            strokeWidth={1.75}
                            className="absolute text-text-secondary opacity-0 transition-opacity group-hover:opacity-100"
                        />
                    </button>
                ) : (
                    <>
                        <img
                            src={logoSidebar}
                            alt="NapCatQQ-Desktop logo"
                            width={24}
                            height={24}
                            className={cn('h-6 w-6 shrink-0', LOGO_IMG_CLASS)}
                            draggable={false}
                        />
                        <span className="whitespace-nowrap font-display text-[13.5px] font-semibold leading-none tracking-tight text-text">
                            NapCatQQ-Desktop
                        </span>
                        <div className="h-full flex-1" data-tauri-drag-region />
                        <button
                            type="button"
                            onClick={onToggleCollapse}
                            aria-label="折叠侧栏"
                            title="折叠侧栏"
                            className={cn(
                                'inline-flex h-6 w-6 items-center justify-center rounded-xs',
                                'text-text-disabled transition-colors hover:bg-text/5 hover:text-text-secondary',
                                'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-brand',
                            )}
                        >
                            <MotionIcon
                                icon={ChevronsLeft}
                                motion="none"
                                hoverAccent
                                size={13}
                                strokeWidth={1.75}
                            />
                        </button>
                    </>
                )}
            </div>

            <div className="my-2 h-px bg-border-subtle" />

            <nav ref={navRef} className="relative flex min-h-0 flex-1 flex-col px-2 pb-3">
                <span
                    ref={indicatorRef}
                    aria-hidden
                    style={{ visibility: 'hidden', opacity: 0 }}
                    className="pointer-events-none absolute left-2 top-0 w-[2px] rounded-r-pill bg-brand"
                />

                <ul className="space-y-0.5">
                    {mainNavItems.map((item) => (
                        <NavRow
                            key={item.id}
                            item={item}
                            isActive={active === item.id}
                            collapsed={collapsed}
                            onSelect={onChange}
                        />
                    ))}
                </ul>

                <ul className="mt-auto space-y-0.5 border-t border-border-subtle pt-2">
                    <TaskQueueNavRow
                        item={TASKS_NAV}
                        isActive={active === 'tasks'}
                        collapsed={collapsed}
                        activeCount={taskQueueActiveCount}
                        onSelect={onChange}
                    />
                    <NavRow
                        item={SETTINGS_NAV}
                        isActive={active === 'settings'}
                        collapsed={collapsed}
                        onSelect={onChange}
                    />
                </ul>
            </nav>
        </aside>
    );
};

interface NavRowProps {
    item: NavItem;
    isActive: boolean;
    collapsed: boolean;
    onSelect: (id: AppRoute) => void;
}

const NavRow: React.FC<NavRowProps> = ({ item, isActive, collapsed, onSelect }) => {
    const Icon = item.icon;
    const iconSize = collapsed ? 20 : 15;

    return (
        <li>
            <button
                type="button"
                onClick={() => onSelect(item.id)}
                aria-current={isActive ? 'page' : undefined}
                title={collapsed ? item.label : undefined}
                className={cn(
                    'group relative flex w-full items-center gap-2.5 rounded-sm px-2.5',
                    'text-[13.5px] font-medium transition-colors',
                    'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-brand',
                    collapsed ? 'h-10 justify-center px-0' : 'h-9',
                    isActive
                        ? 'text-text'
                        : 'text-text-tertiary hover:bg-text/5 hover:text-text-secondary',
                )}
            >
                <MotionIcon
                    icon={Icon}
                    motion={isActive ? NAV_ROUTE_MOTION[item.id] : 'none'}
                    playEnter={isActive}
                    enterKey={isActive ? item.id : undefined}
                    size={iconSize}
                    strokeWidth={1.75}
                    className={cn('shrink-0', isActive && 'text-brand')}
                />
                {!collapsed && <span className="truncate">{item.label}</span>}
            </button>
        </li>
    );
};

interface TaskQueueNavRowProps {
    item: NavItem;
    isActive: boolean;
    collapsed: boolean;
    activeCount: number;
    onSelect: (id: AppRoute) => void;
}

const TaskQueueNavRow: React.FC<TaskQueueNavRowProps> = ({
    item,
    isActive,
    collapsed,
    activeCount,
    onSelect,
}) => {
    const busy = activeCount > 0 && !isActive;
    const iconSize = collapsed ? 20 : 15;
    const label =
        activeCount > 0 && !collapsed ? `任务 (${activeCount})` : item.label;
    const Icon = item.icon;

    return (
        <li>
            <button
                type="button"
                onClick={() => onSelect(item.id)}
                aria-current={isActive ? 'page' : undefined}
                title={collapsed ? (activeCount > 0 ? `任务 (${activeCount})` : item.label) : undefined}
                aria-label={
                    activeCount > 0
                        ? `任务队列，${activeCount} 个进行中`
                        : '任务队列'
                }
                className={cn(
                    'group relative flex w-full items-center gap-2.5 rounded-sm px-2.5',
                    'text-[13.5px] font-medium transition-colors',
                    'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-brand',
                    collapsed ? 'h-10 justify-center px-0' : 'h-9',
                    isActive
                        ? 'text-text'
                        : 'text-text-tertiary hover:bg-text/5 hover:text-text-secondary',
                )}
            >
                <span className="relative inline-flex shrink-0">
                    {busy ? (
                        <MotionIcon
                            key="tasks-busy"
                            icon={Loader2}
                            motion="spin"
                            playEnter={false}
                            size={iconSize}
                            strokeWidth={1.75}
                            className="text-brand"
                        />
                    ) : (
                        <MotionIcon
                            key="tasks-idle"
                            icon={Icon}
                            motion={isActive ? NAV_ROUTE_MOTION[item.id] : 'none'}
                            playEnter={isActive}
                            enterKey={isActive ? item.id : undefined}
                            size={iconSize}
                            strokeWidth={1.75}
                            className={cn('shrink-0', isActive && 'text-brand')}
                        />
                    )}
                    {activeCount > 0 && (
                        <span
                            className={cn(
                                'absolute flex items-center justify-center rounded-pill bg-brand font-medium leading-none text-white',
                                collapsed
                                    ? '-right-1 -top-1 h-3.5 min-w-3.5 px-0.5 text-[9px]'
                                    : '-right-1.5 -top-1 h-4 min-w-4 px-1 text-[10px]',
                            )}
                            aria-hidden
                        >
                            {activeCount > 9 ? '9+' : activeCount}
                        </span>
                    )}
                </span>
                {!collapsed && <span className="truncate">{label}</span>}
            </button>
        </li>
    );
};

export default Sidebar;