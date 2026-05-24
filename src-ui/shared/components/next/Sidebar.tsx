// 左侧导航栏（next）。视觉目标：紧凑、安静、不抢戏。
//
//   ┌─────────────────────┐
//   │ ◆ NapCatQQ-Desktop ⏴ │  ← 48px 高头，logo 与右侧 TitleBar 控制键平齐
//   ├─────────────────────┤
//   │ ▦  Overview         │  ← 36px nav item，active 左侧 2px brand 渐变线
//   │ 🤖  Bots             │
//   │ 🖥  Remote          │
//   │ ⏱  Events           │
//   │ ⚙  Settings         │
//   │ ─────  dev  ─────   │  ← 居中细线 + 小字
//   │ ✨  Showcase        │
//   └─────────────────────┘
//
// 折叠态 (56px)：logo 居中 + 整个 logo 即展开按钮（hover 时显示 ChevronsRight 暗示）。
// 不再保留底部的独立展开按钮，避免"折叠后展开键跑到左下角"这种反人类布局。

import React from 'react';
import {
    Activity,
    Bot,
    ChevronsLeft,
    ChevronsRight,
    LayoutDashboard,
    type LucideIcon,
    Package,
    Server,
    Settings,
    Sparkles,
} from 'lucide-react';
import { cn } from '../../utils/cn';
import logoPng from '../../../assets/logo.png';

export type AppRoute =
    | 'overview'
    | 'bots'
    | 'components'
    | 'remote'
    | 'events'
    | 'settings'
    | 'showcase';

interface SidebarProps {
    active: AppRoute;
    onChange: (route: AppRoute) => void;
    collapsed: boolean;
    onToggleCollapse: () => void;
    showShowcase?: boolean;
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
    { id: 'remote', label: 'Remote', icon: Server },
    { id: 'events', label: 'Events', icon: Activity },
    { id: 'settings', label: 'Settings', icon: Settings },
];

export const Sidebar: React.FC<SidebarProps> = ({
    active,
    onChange,
    collapsed,
    onToggleCollapse,
    showShowcase,
}) => {
    return (
        <aside
            className={cn(
                'relative z-20 flex shrink-0 flex-col bg-sidebar',
                'transition-[width] duration-200 ease-out',
                collapsed ? 'w-14' : 'w-52',
            )}
        >
            {/* Header: logo + brand + collapse 折叠键。
                整行高 48px（h-12）和右侧 TitleBar 同高，logo 与窗口控制键水平对齐。
                文字 nowrap 防止 NapCatQQ-Desktop 这种长名字换行。
                折叠态：整个 header 居中，并且 logo 自身就是展开按钮。 */}
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
                        {/* 默认显 logo，hover 时淡出换 ChevronsRight 暗示能展开 */}
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

                        {/* spacer + drag region：展开态 sidebar 顶部留白处可拖窗 */}
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

            <nav className="flex-1 space-y-0.5 px-2">
                <ul className="space-y-0.5">
                    {PRIMARY_NAV.map((item) => (
                        <NavRow
                            key={item.id}
                            item={item}
                            isActive={active === item.id}
                            collapsed={collapsed}
                            onSelect={onChange}
                        />
                    ))}
                </ul>

                {showShowcase && (
                    <>
                        <DevDivider collapsed={collapsed} />
                        <ul className="space-y-0.5">
                            <NavRow
                                item={{ id: 'showcase', label: 'Showcase', icon: Sparkles }}
                                isActive={active === 'showcase'}
                                collapsed={collapsed}
                                onSelect={onChange}
                            />
                        </ul>
                    </>
                )}
            </nav>

            {/* 折叠态展开入口已经移到顶部 logo（点 logo / 显 ChevronsRight），
                不再保留底部按钮。 */}
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
                {/* active 强调：左 2px brand 渐变线 + 文字提色 */}
                {isActive && (
                    <span
                        aria-hidden
                        className="pointer-events-none absolute inset-y-1.5 left-0 w-[2px] rounded-r-pill bg-brand"
                    />
                )}

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

const DevDivider: React.FC<{ collapsed: boolean }> = ({ collapsed }) => (
    <div
        className={cn(
            'my-3 flex items-center gap-2 px-2 text-text-disabled',
            collapsed && 'px-0',
        )}
        aria-hidden
    >
        <span className="h-px flex-1 bg-border-subtle" />
        {!collapsed && (
            <span className="text-[10px] font-medium uppercase tracking-widest">dev</span>
        )}
        <span className="h-px flex-1 bg-border-subtle" />
    </div>
);

export default Sidebar;
