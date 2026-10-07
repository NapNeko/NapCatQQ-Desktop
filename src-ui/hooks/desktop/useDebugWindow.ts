// 调试台独立工具窗的入口：开弹窗、探它开没开、弹窗首帧 reveal。
// 没有 close：弹窗靠系统窗口按钮关，后端没有对应命令。
// isDebugPopoutWindow 读的是 main.tsx 启动时按窗口 label 写好的模块级标识
// （弹窗与主窗不共享 JS 世界），纯查询无 React 状态，原样透出给界面用。

import { useCallback, useMemo } from 'react';
import { debugWindowService } from '../../core/services/debug-window.service';

export { isDebugPopoutWindow } from '../../core/services/debug-window.service';

export function useDebugWindow() {
    const open = useCallback(() => debugWindowService.open(), []);
    const reveal = useCallback(() => debugWindowService.reveal(), []);
    const focusIfOpen = useCallback(() => debugWindowService.focusIfOpen(), []);

    return useMemo(() => ({ open, reveal, focusIfOpen }), [open, reveal, focusIfOpen]);
}
