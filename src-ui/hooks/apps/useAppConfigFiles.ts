// 配置文件读写；草稿按实例和文件留在查询缓存中，切换页面不会丢失。
import { useIsMutating, useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { appFrameworkService } from '../../core/services/app-framework.service';
import { toAppConfigError } from '../../core/domain/apps/appConfigError';
import { dismissInfoBar } from '../ui/globalInfoBarStore';
import { pushErrorBar } from '../ui/pushErrorBar';
import type { AppConfigDocument, AppConfigText } from '../../core/ipc/types';

const documentsKey = (id: string) => ['appConfigDocuments', id] as const;

function reportConfigError(key: string, title: string, error: unknown) {
    const { message } = toAppConfigError(error);
    pushErrorBar({ key, title, raw: message, content: message });
}

export function useAppConfigDocuments(instanceId: string) {
    const noticeKey = `app-config-documents:${instanceId}`;
    return useQuery({
        queryKey: documentsKey(instanceId),
        queryFn: async ({ signal }) => {
            try {
                const result = await appFrameworkService.listConfigDocuments(instanceId);
                if (!signal.aborted) dismissInfoBar(`key:${noticeKey}`);
                return result;
            } catch (error) {
                if (!signal.aborted) reportConfigError(noticeKey, '读取配置文件列表失败', error);
                throw error;
            }
        },
        retry: false,
    });
}

interface FileDraft {
    base: AppConfigText;
    text: string;
}

export function useAppConfigFile(instanceId: string, document: AppConfigDocument) {
    const client = useQueryClient();
    const fileKey = ['appConfigText', instanceId, document.id, document.rel_path] as const;
    const draftKey = ['appConfigDraft', instanceId, document.id, document.rel_path] as const;
    const noticeKey = `app-config-file:${instanceId}:${document.id}:${document.rel_path}`;
    const file = useQuery({
        queryKey: fileKey,
        queryFn: async ({ signal }) => {
            try {
                const result = await appFrameworkService.readConfigText(instanceId, document.id);
                if (!signal.aborted) dismissInfoBar(`key:${noticeKey}`);
                return result;
            } catch (error) {
                if (!signal.aborted) {
                    reportConfigError(noticeKey, `读取 ${document.label} 失败`, error);
                }
                throw error;
            }
        },
        retry: false,
        refetchOnWindowFocus: false,
    });
    const draft = useQuery<FileDraft | null>({
        queryKey: draftKey,
        queryFn: () => null,
        initialData: null,
        enabled: false,
        gcTime: Infinity,
    });
    const base = draft.data?.base ?? file.data;
    const text = draft.data?.text ?? base?.text ?? '';
    const dirty = !!draft.data && text !== draft.data.base.text;

    const save = useMutation({
        mutationKey: [...fileKey, 'save'],
        onMutate: () => client.cancelQueries({ queryKey: fileKey }),
        mutationFn: async (overwrite: boolean) => {
            if (!base) throw new Error('配置文件尚未加载');
            return appFrameworkService.writeConfigText(
                instanceId,
                document.id,
                text,
                overwrite ? null : base.revision,
            );
        },
        onSuccess: (result) => {
            dismissInfoBar(`key:${noticeKey}`);
            client.setQueryData(fileKey, result);
            client.setQueryData(draftKey, null);
            void client.invalidateQueries({ queryKey: documentsKey(instanceId) });
            void client.invalidateQueries({ queryKey: ['appWebUiAccount', instanceId] });
        },
        onError: (error) => {
            if (toAppConfigError(error).kind !== 'conflict') {
                reportConfigError(noticeKey, `保存 ${document.label} 失败`, error);
            }
        },
    });
    const reload = useMutation({
        mutationKey: [...fileKey, 'reload'],
        onMutate: () => client.cancelQueries({ queryKey: fileKey }),
        mutationFn: () => appFrameworkService.readConfigText(instanceId, document.id),
        onSuccess: (result) => {
            dismissInfoBar(`key:${noticeKey}`);
            client.setQueryData(fileKey, result);
            client.setQueryData(draftKey, null);
            save.reset();
        },
        onError: (error) => reportConfigError(noticeKey, `重新加载 ${document.label} 失败`, error),
    });
    const pendingWrites = useIsMutating({ mutationKey: fileKey });

    return {
        base,
        text,
        dirty,
        isLoading: file.isLoading,
        busy: pendingWrites > 0 || save.isPending || reload.isPending,
        error:
            save.error || reload.error || file.error
                ? toAppConfigError(save.error ?? reload.error ?? file.error)
                : null,
        change: (next: string) => {
            if (!base) return;
            client.setQueryData<FileDraft | null>(
                draftKey,
                next === base.text ? null : { base, text: next },
            );
            save.reset();
            reload.reset();
        },
        save: (overwrite = false) => save.mutateAsync(overwrite),
        reload: () => reload.mutateAsync(),
    };
}
