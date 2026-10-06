// 「另存完整内容」：超过 5 MiB 的回包界面上只显示开头一段，完整内容另存成文件。
// 用户在等结果：成功给一条提示，失败（后端已经不留这次的全文、写文件失败）弹错误条，取消了什么都不说。

import { useMutation } from '@tanstack/react-query';
import { onebotDebugService } from '../../core/services/onebot-debug.service';
import { errorText } from '../../core/domain/errors';
import { responseFileName } from '../../core/domain/debug/responseView';
import { pushErrorBar } from '../ui/pushErrorBar';
import { pushInfoBar } from '../ui/globalInfoBarStore';

export interface SaveResponseArgs {
    requestId: string;
    /** 默认文件名用 */
    action: string;
}

export function useSaveResponse() {
    return useMutation<boolean, unknown, SaveResponseArgs>({
        mutationFn: ({ requestId, action }) =>
            onebotDebugService.saveResponseFile(requestId, responseFileName(action, Date.now())),
        onSuccess: (saved) => {
            if (!saved) return;
            pushInfoBar({
                key: 'debug-save-response',
                tone: 'success',
                title: '完整回包已保存',
                autoDismissMs: 2500,
            });
        },
        onError: (err) => {
            pushErrorBar({
                key: 'debug-save-response',
                title: '另存完整回包失败',
                raw: errorText(err),
            });
        },
    });
}
