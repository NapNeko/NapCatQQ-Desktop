// 配置文件读写；草稿按实例和文件留在查询缓存中，切换页面不会丢失。
import { useIsMutating, useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { appFrameworkService } from '../../core/services/app-framework.service';
import { toAppConfigError } from '../../core/domain/apps/appConfigError';
import type { AppConfigDocument, AppConfigText } from '../../core/ipc/types';

const documentsKey = (id: string) => ['appConfigDocuments', id] as const;

export function useAppConfigDocuments(instanceId: string) {
    return useQuery({
        queryKey: documentsKey(instanceId),
        queryFn: () => appFrameworkService.listConfigDocuments(instanceId),
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
    const file = useQuery({
        queryKey: fileKey,
        queryFn: () => appFrameworkService.readConfigText(instanceId, document.id),
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
            client.setQueryData(fileKey, result);
            client.setQueryData(draftKey, null);
            void client.invalidateQueries({ queryKey: documentsKey(instanceId) });
            void client.invalidateQueries({ queryKey: ['appWebUiAccount', instanceId] });
        },
    });
    const reload = useMutation({
        mutationKey: [...fileKey, 'reload'],
        onMutate: () => client.cancelQueries({ queryKey: fileKey }),
        mutationFn: () => appFrameworkService.readConfigText(instanceId, document.id),
        onSuccess: (result) => {
            client.setQueryData(fileKey, result);
            client.setQueryData(draftKey, null);
            save.reset();
        },
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
