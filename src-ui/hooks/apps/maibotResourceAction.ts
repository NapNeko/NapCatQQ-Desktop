// 麦麦资源页（表达方式、黑话、人物、表情包…）共用的操作 mutation：成功后让这块的列表和概况一起刷，
// 失败走错误条。

import { useMutation, useQueryClient } from '@tanstack/react-query';
import { toAppConfigError } from '../../core/domain/apps/appConfigError';
import { errorText } from '../../core/domain/errors';
import { pushInfoBar } from '../ui/globalInfoBarStore';
import { pushErrorBar } from '../ui/pushErrorBar';
import type { MaiBotResourceDone } from '../../core/ipc/types';

/** 挑本机文件、读拖进来的文件（试聊的图、表情包上传、知识库导入）：只碰本机，不碰实例。
 *  失败弹错误条、当什么都没挑，页面上不用各自兜 */
export function localFilesOrNothing<T>(run: Promise<T[]>, key: string, title: string): Promise<T[]> {
    return run.catch((err: unknown): T[] => {
        pushErrorBar({ key, title, raw: errorText(err) });
        return [];
    });
}

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
            pushErrorBar({ key: `${String(key[0])}-fail:${instanceId}`, title: failTitle, raw: toAppConfigError(err).message });
        },
    });
}
