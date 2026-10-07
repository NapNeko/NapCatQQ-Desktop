// 调试台参数里的本机文件操作：弹系统对话框挑一个本机文件，取消返回 null。
// 挑完只是拿到路径，落成 `ncd-local-file://` 占位的换算留在调用方
// （core/domain/debug/streamActions 的 localFileTokenFor）；这里只管对话框调用和失败弹条。

import { useMutation } from '@tanstack/react-query';
import { onebotDebugService } from '../../core/services/onebot-debug.service';
import { errorText } from '../../core/domain/errors';
import { pushErrorBar } from '../ui/pushErrorBar';

export function useDebugFileOps() {
    const pickLocalFile = useMutation<{ path: string; name: string } | null, unknown>({
        mutationFn: () => onebotDebugService.pickLocalFile(),
        onError: (err) => {
            pushErrorBar({
                key: 'debug-local-file',
                title: '选本机文件失败',
                raw: errorText(err),
            });
        },
    });

    return { pickLocalFile };
}
