// 应用端框架的「可安装版本」查询。
//
// 后端 AppFrameworkAdapter::available_versions 对不支持的框架返回 null，
// 这里原样透出：调用方拿到 null 就不要显示版本选择器。

import { useQuery } from '@tanstack/react-query';
import { appFrameworkService } from '../../core/services/app-framework.service';
import type { PackageVersions } from '../../core/ipc/types';

export const appFrameworkVersionsKey = (frameworkId: string) =>
    ['appFrameworkVersions', frameworkId] as const;

/**
 * 某框架的可安装版本；null = 该框架不支持按版本安装。
 *
 * 缓存时间给得长一些：PyPI 版本清单变化不频繁，而打开对话框 / 详情页会反复要。
 */
export function useAppFrameworkVersions(frameworkId: string | null | undefined) {
    return useQuery<PackageVersions | null, Error>({
        queryKey: appFrameworkVersionsKey(frameworkId ?? ''),
        queryFn: () => appFrameworkService.listVersions(frameworkId as string),
        enabled: !!frameworkId,
        staleTime: 10 * 60 * 1000,
        retry: false,
    });
}
