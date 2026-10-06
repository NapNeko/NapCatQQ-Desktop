// 新建、导入实例对话框要向后端问的两件事：默认装到哪，导入前那个目录是不是个能接管的项目。

import { keepPreviousData, useMutation, useQuery } from '@tanstack/react-query';
import { appFrameworkService } from '../../core/services/app-framework.service';
import type { AppProjectProbe } from '../../core/ipc/types';

/** 默认安装的父目录（实例目录名装的时候才编号）。换主机时先留着上一台的，拉不到就空着，不拦新建 */
export function useAppInstallDirPreview(hostId: string | null, frameworkId: string | null): string {
    const query = useQuery<string, Error>({
        queryKey: ['appInstallDirPreview', hostId ?? '', frameworkId ?? ''],
        queryFn: () => appFrameworkService.previewInstallDir(hostId!, frameworkId!),
        enabled: !!hostId && !!frameworkId,
        placeholderData: keepPreviousData,
    });
    return query.isError ? '' : (query.data ?? '');
}

/** 检查结果只给发起的对话框用，不进缓存；失败的提示由对话框自己弹，它要跟着改路径一起收掉 */
export function useProbeAppProject() {
    return useMutation<
        AppProjectProbe,
        unknown,
        { hostId: string; frameworkId: string; path: string }
    >({
        mutationFn: ({ hostId, frameworkId, path }) =>
            appFrameworkService.probeProject(hostId, frameworkId, path),
    });
}
