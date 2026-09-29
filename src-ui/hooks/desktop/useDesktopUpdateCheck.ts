// 设置页「关于」里手动检查 Desktop 新版本。结果只给这一页用，不进缓存；
// 装更新走组件动作（desktop_self），进度在任务队列。

import { useMutation } from '@tanstack/react-query';
import { desktopUpdateService } from '../../core/services/desktop-update.service';

export function useDesktopUpdateCheck() {
    const check = useMutation({ mutationFn: () => desktopUpdateService.check() });
    return check.mutateAsync;
}
