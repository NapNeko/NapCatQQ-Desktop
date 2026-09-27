// 一格终端：把会话的 xterm 挂进来、跟着大小改行列；右键菜单、多行粘贴确认、sudo 代填按钮、搜索条都在这一格上。

import { useCallback, useEffect, useRef, useState } from 'react';
import { ClipboardCopy, ClipboardPaste, Eraser, KeyRound, Search, TextSelect } from 'lucide-react';
import {
    Button,
    ContextMenu,
    ContextMenuContent,
    ContextMenuItem,
    ContextMenuSeparator,
    ContextMenuShortcut,
    ContextMenuTrigger,
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from '../../shared/ui';
import { terminalIo } from '../../hooks/terminal/terminalIo';
import { terminalStore, useTerminalSession } from '../../hooks/terminal/terminalStore';
import { getRuntime } from './registry';
import { TerminalSearchBar } from './TerminalSearchBar';

interface Props {
    sessionId: string;
    focused: boolean;
    /** 拖文件悬停在这一格上 */
    dropHint: string | null;
}

interface PendingPaste {
    text: string;
    resolve(ok: boolean): void;
}

export function TerminalView({ sessionId, focused, dropHint }: Props) {
    const containerRef = useRef<HTMLDivElement>(null);
    const session = useTerminalSession(sessionId);
    const runtime = getRuntime(sessionId);
    const [searchOpen, setSearchOpen] = useState(false);
    const [pendingPaste, setPendingPaste] = useState<PendingPaste | null>(null);
    const [hasSelection, setHasSelection] = useState(false);

    const confirmPaste = useCallback(
        (text: string) => new Promise<boolean>((resolve) => setPendingPaste({ text, resolve })),
        [],
    );

    useEffect(() => {
        const el = containerRef.current;
        if (!runtime || !el) return;
        runtime.view = { confirmPaste, openSearch: () => setSearchOpen(true) };
        runtime.mount(el);
        const observer = new ResizeObserver(() => runtime.fitNow());
        observer.observe(el);
        return () => {
            observer.disconnect();
            if (runtime.view?.confirmPaste === confirmPaste) runtime.view = null;
            runtime.unmount();
        };
    }, [runtime, confirmPaste]);

    useEffect(() => {
        if (focused) runtime?.focus();
    }, [focused, runtime]);

    const answerPaste = (ok: boolean) => {
        pendingPaste?.resolve(ok);
        setPendingPaste(null);
        runtime?.focus();
    };

    if (!runtime || !session) return null;
    const lines = pendingPaste ? pendingPaste.text.replace(/[\r\n]+$/, '').split(/\r\n|\r|\n/) : [];

    return (
        <>
            <ContextMenu
                onOpenChange={(open) => {
                    if (open) setHasSelection(runtime.hasSelection());
                }}
            >
                <ContextMenuTrigger asChild>
                    <div
                        ref={containerRef}
                        className="relative min-h-0 min-w-0 flex-1"
                        onMouseDown={() => {
                            if (!focused) terminalStore.focusPane(sessionId);
                        }}
                    />
                </ContextMenuTrigger>
                <ContextMenuContent className="min-w-[180px]">
                    <ContextMenuItem disabled={!hasSelection} onClick={() => runtime.copySelection()}>
                        <ClipboardCopy size={13} />
                        <span>复制</span>
                        <ContextMenuShortcut>Ctrl+C</ContextMenuShortcut>
                    </ContextMenuItem>
                    <ContextMenuItem onClick={() => void runtime.pasteFromClipboard()}>
                        <ClipboardPaste size={13} />
                        <span>粘贴</span>
                        <ContextMenuShortcut>Ctrl+V</ContextMenuShortcut>
                    </ContextMenuItem>
                    <ContextMenuItem onClick={() => runtime.selectAll()}>
                        <TextSelect size={13} />
                        <span>全选</span>
                    </ContextMenuItem>
                    <ContextMenuSeparator />
                    <ContextMenuItem onClick={() => setSearchOpen(true)}>
                        <Search size={13} />
                        <span>搜索</span>
                        <ContextMenuShortcut>Ctrl+Shift+F</ContextMenuShortcut>
                    </ContextMenuItem>
                    <ContextMenuItem onClick={() => runtime.clear()}>
                        <Eraser size={13} />
                        <span>清屏</span>
                    </ContextMenuItem>
                </ContextMenuContent>
            </ContextMenu>

            {searchOpen && <TerminalSearchBar runtime={runtime} onClose={() => setSearchOpen(false)} />}

            {session.sudoPrompt && (
                <div className="absolute bottom-3 right-5 z-20">
                    <Button
                        size="sm"
                        variant="primary"
                        onClick={() => {
                            void terminalIo.fillSudo(sessionId);
                            terminalStore.setSudoPrompt(sessionId, false);
                            runtime.focus();
                        }}
                    >
                        <KeyRound size={13} />
                        填入 sudo 密码
                    </Button>
                </div>
            )}

            {dropHint && <div className="ncd-term-drop">{dropHint}</div>}

            <Dialog open={pendingPaste !== null} onOpenChange={(open) => !open && answerPaste(false)}>
                <DialogContent size="lg">
                    <DialogHeader>
                        <DialogTitle>粘贴 {lines.length} 行？</DialogTitle>
                        <DialogDescription>多行文本贴进去会一行一行当命令执行。</DialogDescription>
                    </DialogHeader>
                    <pre className="max-h-60 overflow-auto rounded-sm bg-inset p-3 font-mono text-[12px] leading-[18px] text-text">
                        {lines.slice(0, 200).join('\n')}
                        {lines.length > 200 ? `\n…（还有 ${lines.length - 200} 行）` : ''}
                    </pre>
                    <DialogFooter>
                        <Button variant="ghost" onClick={() => answerPaste(false)}>
                            不贴
                        </Button>
                        <Button variant="primary" onClick={() => answerPaste(true)}>
                            粘贴
                        </Button>
                    </DialogFooter>
                </DialogContent>
            </Dialog>
        </>
    );
}
