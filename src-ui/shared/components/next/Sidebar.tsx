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
    type LucideIcon,
    Package,
    Server,
    Settings,
} from 'lucide-react';
import gsap from 'gsap';
import { cn } from '../../utils/cn';
import { useMotion } from '../../../hooks/preferences/useMotion';
import logoPng from '../../../assets/logo.png';

export type AppRoute =
    | 'overview'
    | 'bots'
    | 'components'
    | 'docker'
    | 'remote'
    | 'settings';

interface SidebarProps {
    active: AppRoute;
    onChange: (route: AppRoute) => void;
    collapsed: boolean;
    onToggleCollapse: () => void;
    /// 是否显示 Docker 项。
    showDocker?: boolean;
}

interface NavItem {
    id: AppRoute;
    label: string;
    icon: LucideIcon;
}

const PRIMARY_NAV: NavItem[] = [
    { id: 'overview', label: 'Overview', icon: LayoutDashboard },
    { id: 'bots', label: 'Bots', icon: Bot },
    { id: 'components', label: 'Components', icon: Package },
    { id: 'docker', label: 'Docker', icon: Container },
    { id: 'remote', label: 'Remote', icon: Server },
    { id: 'settings', label: 'Settings', icon: Settings },
];

export const Sidebar: React.FC<SidebarProps> = ({
    active,
    onChange,
    collapsed,
    onToggleCollapse,
    showDocker = true,
}) => {
    const navItems = showDocker
        ? PRIMARY_NAV
        : PRIMARY_NAV.filter((item) => item.id !== 'docker');

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
                            src={logoPng}
                            alt="NapCatQQ-Desktop logo"
                            className="h-5 w-5 select-none transition-opacity group-hover:opacity-0"
                            draggable={false}
                        />
                        <ChevronsRight
                            size={16}
                            strokeWidth={1.75}
                            className="absolute text-text-secondary opacity-0 transition-opacity group-hover:opacity-100"
                        />
                    </button>
                ) : (
                    <>
                        <img
                            src={logoPng}
                            alt="NapCatQQ-Desktop logo"
                            className="h-[20px] w-[20px] shrink-0 select-none"
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
                            <ChevronsLeft size={13} strokeWidth={1.75} />
                        </button>
                    </>
                )}
            </div>

            <div className="my-2 h-px bg-border-subtle" />

            <nav ref={navRef} className="relative flex-1 space-y-0.5 px-2">
                {/* FLIP indicator:absolute 定位,GSAP 控 y + height。 */}
                <span
                    ref={indicatorRef}
                    aria-hidden
                    style={{ visibility: 'hidden', opacity: 0 }}
                    className="pointer-events-none absolute left-2 top-0 w-[2px] rounded-r-pill bg-brand"
                />

                <ul className="space-y-0.5">
                    {navItems.map((item) => (
                        <NavRow
                            key={item.id}
                            item={item}
                            isActive={active === item.id}
                            collapsed={collapsed}
                            onSelect={onChange}
                        />
                    ))}
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
    return (
        <li>
            <button
                type="button"
                onClick={() => onSelect(item.id)}
                aria-current={isActive ? 'page' : undefined}
                title={collapsed ? item.label : undefined}
                className={cn(
                    'group relative flex h-9 w-full items-center gap-2.5 rounded-sm px-2.5',
                    'text-[13.5px] font-medium transition-colors',
                    'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-brand',
                    isActive
                        ? 'text-text'
                        : 'text-text-tertiary hover:bg-text/5 hover:text-text-secondary',
                    collapsed && 'justify-center px-0',
                )}
            >
                <Icon
                    size={15}
                    strokeWidth={1.75}
                    className={cn('shrink-0', isActive && 'text-brand')}
                />
                {!collapsed && <span className="truncate">{item.label}</span>}
            </button>
        </li>
    );
};

export default Sidebar;
