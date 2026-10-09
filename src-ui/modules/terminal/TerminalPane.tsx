// 标签里的一块终端：窄标题行（名字、主机、当前目录、状态、按钮）+ 终端本体 + 文件栏 + 服务器状态条。

import {
    Blocks,
    Bot,
    Eraser,
    FileDown,
    FolderTree,
    Monitor,
    MoreHorizontal,
    RotateCcw,
    Search,
    Server,
    SquareArrowOutUpRight,
    X,
} from 'lucide-react';
import { useRef } from 'react';
import { Popover, PopoverContent, PopoverTrigger } from '../../shared/ui';
import { cn } from '../../shared/utils/cn';
import { terminalLayout, useTerminalLayout } from '../../hooks/terminal/terminalPrefs';
import {
    isLive,
    sessionTitle,
    terminalStore,
    useTerminalSession,
} from '../../hooks/terminal/terminalStore';
import { terminalIo } from '../../hooks/terminal/terminalIo';
import { useTerminalExternal } from '../../hooks/terminal/useTerminalExternal';
import { targetKey } from '../../core/domain/terminal/commands';
import { cdCommand, quotePath, shellSyntaxOf } from '../../core/domain/terminal/paths';
import type { TerminalStatus } from '../../core/ipc/generated/domain/TerminalStatus';
import type { TerminalTarget } from '../../core/ipc/generated/domain/TerminalTarget';
import { getRuntime } from './registry';
import { TerminalCommandsMenu } from './TerminalCommandsMenu';
import { TerminalFilesPanel } from './TerminalFilesPanel';
import { TerminalStatsBar } from './TerminalStatsBar';
import { TerminalView } from './TerminalView';

export function TargetIcon({ target, size = 12 }: { target: TerminalTarget; size?: number }) {
    const Icon =
        target.kind === 'local'
            ? Monitor
            : target.kind === 'server'
              ? Server
              : target.kind === 'bot'
                ? Bot
                : Blocks;
    return <Icon size={size} className="shrink-0" />;
}

function statusText(status: TerminalStatus): string | null {
    switch (status.kind) {
        case 'exited':
            return status.code === undefined ? '已结束' : `已退出（${status.code}）`;
        case 'disconnected':
            return '连接断了';
        case 'failed':
            return '没开起来';
        case 'starting':
            return '正在打开…';
        default:
            return null;
    }
}

function HeaderButton({
    title,
    onClick,
    active,
    children,
}: {
    title: string;
    onClick(): void;
    active?: boolean;
    children: React.ReactNode;
}) {
    return (
        <button
            type="button"
            title={title}
            aria-label={title}
            aria-pressed={active}
            onClick={onClick}
            className={cn(
                'flex h-6 w-6 items-center justify-center rounded-xs transition-colors',
                active
                    ? 'bg-accent-soft text-text'
                    : 'text-text-tertiary hover:bg-inset hover:text-text',
            )}
        >
            {children}
        </button>
    );
}

function MenuRow({
    onClick,
    children,
    danger,
}: {
    onClick(): void;
    children: React.ReactNode;
    danger?: boolean;
}) {
    return (
        <button
            type="button"
            onClick={onClick}
            className={cn(
                'flex w-full items-center gap-2 rounded-xs px-2 py-1.5 text-left text-[12px] hover:bg-inset',
                danger ? 'text-danger' : 'text-text',
            )}
        >
            {children}
        </button>
    );
}

interface Props {
    sessionId: string;
    focused: boolean;
    visible: boolean;
    /** 拖文件悬停在哪一块：终端本体 / 文件栏 */
    dropZone: 'terminal' | 'files' | null;
    showHeader: boolean;
}

export function TerminalPane({ sessionId, focused, visible, dropZone, showHeader }: Props) {
    const view = useTerminalSession(sessionId);
    const layout = useTerminalLayout();
    const openExternal = useTerminalExternal();
    // 文件栏只在这块终端里被点开时滑进来；切标签重新挂上时本来就开着的不再滑一遍
    const filesSlide = useRef(false);
    if (!view) return null;
    const { info } = view;
    const live = isLive(info.status);
    const syntax = shellSyntaxOf(info.host_os, info.shell);
    const filesOpen = view.filesOpen && info.features.files;
    if (!filesOpen) filesSlide.current = true;
    const status = statusText(info.status);
    const runtime = getRuntime(sessionId);

    const dropHint =
        dropZone !== 'terminal'
            ? null
            : info.host_id === 'local'
              ? '松手把路径填进终端'
              : info.features.files
                ? `松手传到 ${view.cwd ?? '家目录'}`
                : '容器里的终端没法直接传文件';

    return (
        <div
            className={cn(
                'relative flex min-h-0 min-w-0 flex-1 flex-col',
                focused ? '' : 'opacity-[0.97]',
            )}
            data-terminal-drop={sessionId}
        >
            {showHeader && (
                <div
                    className={cn(
                        'flex h-7 shrink-0 items-center gap-2 border-b border-border-subtle pl-3 pr-1.5 text-[12px]',
                        focused ? 'text-text' : 'text-text-secondary',
                    )}
                >
                    <TargetIcon target={info.target} />
                    <span className="max-w-[40%] shrink-0 truncate font-medium">
                        {sessionTitle(view)}
                    </span>
                    {view.cwd && (
                        <span
                            className="min-w-0 flex-1 truncate font-mono text-[11px] text-text-tertiary"
                            title={view.cwd}
                        >
                            {view.cwd}
                        </span>
                    )}
                    {!view.cwd && <span className="flex-1" />}
                    {status && (
                        <button
                            type="button"
                            onClick={() => void terminalStore.restart(sessionId)}
                            className="ncd-term-pop flex shrink-0 items-center gap-1 rounded-pill bg-warning-soft px-2 py-0.5 text-[11px] text-text hover:brightness-95"
                            title="重新打开（也可以在终端里按回车）"
                        >
                            <RotateCcw size={11} />
                            {status}
                        </button>
                    )}
                    <TerminalCommandsMenu
                        targetKey={targetKey(info.target)}
                        snippets={info.snippets}
                        onPick={(cmd) => runtime?.fillInput(cmd)}
                    />
                    <HeaderButton
                        title="搜索（Ctrl+Shift+F）"
                        onClick={() => runtime?.view?.openSearch()}
                    >
                        <Search size={13} />
                    </HeaderButton>
                    {info.features.files && (
                        <HeaderButton
                            title={filesOpen ? '收起文件栏' : '文件栏'}
                            active={filesOpen}
                            onClick={() => terminalStore.setFilesOpen(sessionId, !view.filesOpen)}
                        >
                            <FolderTree size={13} />
                        </HeaderButton>
                    )}
                    <HeaderButton title="清屏" onClick={() => runtime?.clear()}>
                        <Eraser size={13} />
                    </HeaderButton>
                    <Popover>
                        <PopoverTrigger asChild>
                            <button
                                type="button"
                                title="更多"
                                aria-label="更多"
                                className="flex h-6 w-6 items-center justify-center rounded-xs text-text-tertiary hover:bg-inset hover:text-text"
                            >
                                <MoreHorizontal size={13} />
                            </button>
                        </PopoverTrigger>
                        <PopoverContent align="end" className="w-52 p-1">
                            <MenuRow
                                onClick={() =>
                                    runtime &&
                                    void terminalIo.exportText(
                                        `${sessionTitle(view).replace(/[\\/:*?"<>|\s·]+/g, '-')}.log`,
                                        runtime.bufferText(),
                                    )
                                }
                            >
                                <FileDown size={13} />
                                导出输出
                            </MenuRow>
                            {info.features.external && (
                                <MenuRow onClick={() => openExternal(info.target, info.shell)}>
                                    <SquareArrowOutUpRight size={13} />
                                    在系统终端里打开
                                </MenuRow>
                            )}
                            {!live && (
                                <MenuRow onClick={() => void terminalStore.restart(sessionId)}>
                                    <RotateCcw size={13} />
                                    重新打开
                                </MenuRow>
                            )}
                            <MenuRow danger onClick={() => void terminalStore.close(sessionId)}>
                                <X size={13} />
                                关掉这个终端
                            </MenuRow>
                        </PopoverContent>
                    </Popover>
                </div>
            )}
            {view.progress && (
                <div className="h-0.5 shrink-0 bg-border-subtle">
                    <div
                        className={cn(
                            'h-full transition-[width] duration-300',
                            view.progress.state === 2
                                ? 'bg-danger'
                                : view.progress.state === 4
                                  ? 'bg-warning'
                                  : 'bg-accent',
                            view.progress.state === 3 && 'w-1/3 animate-pulse',
                        )}
                        style={
                            view.progress.state === 3
                                ? undefined
                                : { width: `${view.progress.value}%` }
                        }
                    />
                </div>
            )}
            <div className="flex min-h-0 flex-1">
                <div
                    className="relative flex min-h-0 min-w-0 flex-1 flex-col"
                    style={{ background: 'var(--ncd-term-bg)' }}
                >
                    <TerminalView
                        sessionId={sessionId}
                        focused={focused && visible}
                        dropHint={dropHint}
                    />
                </div>
                {filesOpen && (
                    <TerminalFilesPanel
                        sessionId={sessionId}
                        visible={visible}
                        hostOs={info.host_os}
                        cwd={view.cwd}
                        width={layout.filesWidth}
                        dropping={dropZone === 'files'}
                        slideIn={filesSlide.current}
                        onCd={(path) => runtime?.fillInput(cdCommand(path, syntax))}
                        onInsertPath={(path) => runtime?.fillInput(`${quotePath(path, syntax)} `)}
                        onWidthChange={(filesWidth) => terminalLayout.patch({ filesWidth })}
                    />
                )}
            </div>
            {info.features.stats && <TerminalStatsBar sessionId={sessionId} visible={visible} />}
        </div>
    );
}
