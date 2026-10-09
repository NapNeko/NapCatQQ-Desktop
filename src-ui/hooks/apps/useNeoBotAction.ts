import { useMutation, useQueryClient } from '@tanstack/react-query';
import { appFrameworkService } from '../../core/services/app-framework.service';
import {
    changeMessage,
    fieldErrors,
    NeoBotPanelError,
    record,
    text,
    type PanelObject,
} from '../../core/domain/apps/neobotWorkspace';
import { pushInfoBar } from '../ui/globalInfoBarStore';

export interface NeoBotRequest {
    path: string;
    method?: 'GET' | 'POST' | 'PUT' | 'DELETE';
    body?: unknown;
    quiet?: boolean;
    allowNegative?: boolean;
}

export function useNeoBotAction(instanceId: string) {
    const client = useQueryClient();
    const action = useMutation({
        retry: false,
        mutationFn: async (req: NeoBotRequest): Promise<PanelObject> => {
            const res = await appFrameworkService.panelCall(
                instanceId,
                req.method ?? 'POST',
                req.path,
                req.body,
            );
            const data = record(res?.data);
            if (!res || res.kind !== 'ok' || (data.ok === false && !req.allowNegative)) {
                const detail = fieldErrors(data);
                throw new NeoBotPanelError(
                    detail ||
                        res?.message ||
                        text(data.error) ||
                        text(data.message) ||
                        '面板拒绝了这次操作',
                    res?.status ?? undefined,
                    res?.data,
                );
            }
            return data;
        },
        onSuccess: (data, req) => {
            const reloadRequested = record(req.body).reload === true;
            if (!req.quiet)
                pushInfoBar({
                    key: `neobot:${instanceId}:action`,
                    tone:
                        (reloadRequested && data.applied === false) || data.ok === false
                            ? 'warning'
                            : 'success',
                    title: 'NeoBot',
                    content: changeMessage(data, reloadRequested),
                });
            if ((req.method ?? 'POST') !== 'GET' && !req.quiet)
                void client.invalidateQueries({ queryKey: ['neobotPanel', instanceId] });
        },
        onError: (error) =>
            pushInfoBar({
                key: `neobot:${instanceId}:action`,
                tone: 'danger',
                title: 'NeoBot 操作失败',
                content: error.message,
            }),
    });
    const run = async (req: NeoBotRequest): Promise<PanelObject | null> => {
        try {
            return await action.mutateAsync(req);
        } catch {
            return null;
        }
    };
    return { ...action, run };
}
