// 标题栏上的终端开关：开着几个终端就显示几；后台有终端跑完命令失败时亮红点。

import { SquareTerminal } from 'lucide-react';
import { cn } from '../../utils/cn';
import { terminalStore, useTerminalState } from '../../../hooks/terminal/terminalStore';

export function TerminalToggleButton() {
    const state = useTerminalState();
    const count = Object.keys(state.sessions).length;
    const failed = Object.values(state.sessions).some((s) => s.activity === 'fail');
    return (
        <button
            type="button"
            onClick={() => terminalStore.toggle()}
            title={state.open ? '收起终端（Ctrl+`）' : '终端（Ctrl+`）'}
            aria-label="终端"
            aria-pressed={state.open}
            className={cn(
                'relative flex h-7 items-center gap-1 rounded-md px-1.5 text-text-tertiary',
                'transition-all duration-150 ease-out hover:bg-accent-soft hover:text-text active:scale-95',
                'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent',
                state.open && 'text-text',
            )}
        >
            <SquareTerminal size={14} strokeWidth={2} />
            {count > 0 && <span className="text-[11px] tabular-nums">{count}</span>}
            {failed && !state.open && <span className="absolute right-0.5 top-1 h-1.5 w-1.5 rounded-full bg-danger" />}
        </button>
    );
}
