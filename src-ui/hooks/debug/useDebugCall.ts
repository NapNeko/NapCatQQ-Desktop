// 发一次调试调用，并把「这个标签正在等哪一次、最近一次结果是什么」记进工作区 store 的 runs。
//
// 同一个标签连发时只显示最后一次的结果（防串台）：回包回来时 inflight 还是自己才写 last，
// 否则这次结果只进历史。send 永远不 reject —— 没拿到回包的原因本来就是 response.result 里的数据，
// 连 invoke 本身抛出的错也折成同样的形状，调用方只有一条路径要处理。
//
// 这个 hook 本身不订阅任何 store：中栏、收藏、聊天输入框都挂着它，订了 runs 的话任何一个标签的调用
// 起落都会把它们全刷一遍。要跟着某个标签的调用状态变的界面用 useTabRun(tabId)。

import { useCallback, useMemo } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { onebotDebugService } from '../../core/services/onebot-debug.service';
import { errorText } from '../../core/domain/errors';
import { newRequestId } from '../../core/domain/debug/ids';
import { localFilesInParams, needsStreamCall } from '../../core/domain/debug/streamActions';
import { pushErrorBar } from '../ui/pushErrorBar';
import type { DebugCallRequest } from '../../core/ipc/generated/debug/DebugCallRequest';
import type { DebugCallResponse } from '../../core/ipc/generated/debug/DebugCallResponse';
import type { DebugStreamProgress } from '../../core/ipc/generated/debug/DebugStreamProgress';
import { debugEventStore } from './debugEventStore';
import { debugWorkspaceStore } from './debugWorkspaceStore';
import { debugChannelsKey, debugHistoryPrefix } from './keys';

/** 流式调用的进度写进对应标签的在途调用里；在途换人了（又发了一次）就直接丢 */
function recordProgress(tabId: string, requestId: string, p: DebugStreamProgress): void {
    const run = debugWorkspaceStore.getRun(tabId);
    const cur = run?.inflight;
    if (!run || !cur || cur.requestId !== requestId) return;
    debugWorkspaceStore.setRun(tabId, {
        ...run,
        inflight: { ...cur, progress: p },
    });
}

export function useDebugCall() {
    const client = useQueryClient();

    /**
     * `tabId` 给编辑器标签；聊天输入框、选择器这类不挂在标签上的传 null，只拿返回值。
     * 调用方自己把 `origin` 填对（editor / composer / picker）。
     * 下载动作、或参数里有本机文件占位时自动走流式命令（进度一路上报、可取消）。
     */
    const send = useCallback(
        async (
            tabId: string | null,
            req: Omit<DebugCallRequest, 'request_id'>,
        ): Promise<DebugCallResponse> => {
            const requestId = newRequestId();
            if (tabId !== null) {
                // 保留上一次的结果，新结果回来之前视图不必先空一下
                debugWorkspaceStore.setRun(tabId, {
                    ...debugWorkspaceStore.getRun(tabId),
                    inflight: { requestId, startedAt: Date.now() },
                });
            }

            let response: DebugCallResponse;
            try {
                if (needsStreamCall(req.action, req.params)) {
                    response = await onebotDebugService.callStream(
                        {
                            ...req,
                            request_id: requestId,
                            local_files: localFilesInParams(req.params),
                        },
                        (p) => {
                            if (tabId !== null && p.request_id === requestId)
                                recordProgress(tabId, requestId, p);
                        },
                    );
                } else {
                    response = await onebotDebugService.call({ ...req, request_id: requestId });
                }
            } catch (err) {
                response = {
                    request_id: requestId,
                    result: { kind: 'err', error: { kind: 'internal', message: errorText(err) } },
                };
            }

            // 拿到了回包但 OB11 说失败：事件流里的调用记录不带上游的说明，这里补给聊天时间线。
            // 选择器的查询不进聊天，不必记
            if (
                response.result.kind === 'ok' &&
                !response.result.outcome.ok &&
                req.origin !== 'picker'
            ) {
                const { wording, message } = response.result.outcome;
                const why = wording.trim() || message.trim();
                if (why) debugEventStore.noteCallWording(req.bot_id, requestId, why);
            }

            if (
                tabId !== null &&
                debugWorkspaceStore.getRun(tabId)?.inflight?.requestId === requestId
            ) {
                debugWorkspaceStore.setRun(tabId, {
                    last: { response, at: Date.now(), botId: req.bot_id, action: req.action },
                });
            }
            // 不管这次结果显不显示，它都已经进了历史
            void client.invalidateQueries({ queryKey: debugHistoryPrefix });
            // 每次调用后端都会把通道状态记回会话（鉴权失败、超时……）：「自动」落点和各通道的
            // 可用性可能因此变化。本机 IPC，重拉很便宜；错误卡片上的「查看通道」靠它不说旧话
            void client.invalidateQueries({ queryKey: debugChannelsKey(req.bot_id) });
            return response;
        },
        [client],
    );

    /** 只是不再等；后端会把这次调用记成 cancelled 回来，正常走 send 的收尾 */
    const cancel = useCallback(async (tabId: string): Promise<void> => {
        const inflight = debugWorkspaceStore.getRun(tabId)?.inflight;
        if (!inflight) return;
        try {
            await onebotDebugService.cancel(inflight.requestId);
        } catch (err) {
            pushErrorBar({ key: 'debug-cancel', title: '取消调用失败', raw: errorText(err) });
        }
    }, []);

    /** 此刻这个标签有没有在等的调用。读的是当下的快照、不会让调用方跟着重渲染；要跟着变用 useTabRun */
    const isInflight = useCallback(
        (tabId: string | null): boolean =>
            tabId !== null && !!debugWorkspaceStore.getRun(tabId)?.inflight,
        [],
    );

    return useMemo(() => ({ send, cancel, isInflight }), [send, cancel, isInflight]);
}
