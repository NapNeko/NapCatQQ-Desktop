// 本机目标在系统终端里打开（Windows 11 默认终端是 Windows Terminal 时由它接管），环境和内嵌终端一样。

import { useCallback } from 'react';
import { terminalService } from '../../core/services/terminal.service';
import { errorText } from '../../core/domain/errors';
import { pushErrorBar } from '../ui/pushErrorBar';
import type { LocalShellKind } from '../../core/ipc/generated/domain/LocalShellKind';
import type { TerminalTarget } from '../../core/ipc/generated/domain/TerminalTarget';

export function useTerminalExternal() {
    return useCallback((target: TerminalTarget, shell?: LocalShellKind) => {
        terminalService
            .openExternal({ target, cols: 120, rows: 30, shell })
            .catch((err) => pushErrorBar({ title: '没能打开系统终端', raw: errorText(err) }));
    }, []);
}
