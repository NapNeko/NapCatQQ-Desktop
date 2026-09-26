// 麦麦资源页（表达方式、黑话、人物、表情包…）共用的操作 mutation：成功后让这块的列表和概况一起刷，
// 失败走错误条。

import { useMutation, useQueryClient } from '@tanstack/react-query';
import { toAppConfigError } from '../../core/domain/apps/appConfigError';
import { pushInfoBar } from '../ui/globalInfoBarStore';
import { pushAppErrorBar } from './pushAppErrorBar';
import type { MaiBotResourceDone } from '../../core/ipc/types';

export type ResourceActionOpts = {
    /** 成功时弹一条提示；精选这种一点一下的就不弹，列表变了就是结果 */
    toast?: boolean;
    /** 接在上游回话后面的一句（「下一轮维护后开始发」这种） */
    note?: string;
};

export function useResourceAction<A>(
    instanceId: string,
    key: readonly unknown[],
    run: (instanceId: string, action: A) => Promise<MaiBotResourceDone>,
    failTitle: string,
) {
    const qc = useQueryClient();
    return useMutation<MaiBotResourceDone, unknown, A & ResourceActionOpts>({
        mutationFn: ({ toast: _toast, note: _note, ...action }) => run(instanceId, action as A),
        onSuccess: (res, vars) => {
            void qc.invalidateQueries({ queryKey: key });
            if (vars.toast && res.message) {
                pushInfoBar({
                    key: `${String(key[0])}:${instanceId}`,
                    tone: 'success',
                    title: res.message,
                    content: vars.note,
                    autoDismissMs: vars.note ? 4000 : 2500,
                });
            }
        },
        onError: (err) => {
            pushAppErrorBar({ key: `${String(key[0])}-fail:${instanceId}`, title: failTitle, raw: toAppConfigError(err).message });
        },
    });
}
