// 收口 ChatEmojiPicker 的账号表情目录与最近使用、ChatSearch/QQFace 的名称查询、QQFace 的资源失效。
// QQFace 是性能保护名单:bundled 兜底与租约生命周期留在组件内,这里只做同语义转发。
import { useCallback, useMemo, useRef, useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import type { DebugTarget } from '../../core/ipc/generated/debug/DebugTarget';
import type { QQSystemFace } from '../../core/domain/chat/qqFaces';
import {
    qqFaceService,
    loadRecentQQFaces,
    rememberQQFace,
    type QQFaceCatalog,
} from '../../core/services/qq-face.service';
import {
    qqFaceAssetService,
    type QQFaceImageLease,
} from '../../core/services/qq-face-assets.service';

export { QQ_CLASSIC_FACES } from '../../core/services/qq-face.service';
export { bundledQQFaceSource } from '../../core/services/qq-face-bundled.service';
export type { QQFaceCatalog, QQFaceImageLease };

const faceCatalogKey = (target: DebugTarget) =>
    ['chat', 'qq-faces', target.backend, target.bot_id, String(target.qq_id)] as const;

/**
 * 目录读取走服务自身的账号缓存(10 分钟 / 失败 60 秒),refresh() 强制回源,
 * 与选择器原 forAccount(target, refresh) + faceRefresh 计数的行为一致。
 */
export function useQQFaceCatalog(target: DebugTarget, enabled: boolean) {
    const queries = useQueryClient();
    const key = faceCatalogKey(target);
    const force = useRef(false);
    const query = useQuery({
        queryKey: key,
        queryFn: () => {
            const refresh = force.current;
            force.current = false;
            return qqFaceService.forAccount(target, refresh);
        },
        enabled,
        // 每次启用都向服务要一次,回不回源由服务自己的账号缓存 TTL 决定,与选择器原 effect 时机一致。
        staleTime: 0,
    });
    const refresh = useCallback(() => {
        force.current = true;
        void queries.invalidateQueries({ queryKey: faceCatalogKey(target) });
    }, [target.backend, target.bot_id, target.qq_id, queries]);
    return {
        // 未就绪时回落到账号缓存,首帧不闪空目录,与选择器原 readyCatalog 判定等价。
        catalog: query.data ?? qqFaceService.peekAccount(target),
        isLoading: query.isPending,
        refresh,
    };
}

/** 同步 peek 的稳定包装:投影/标题在每次渲染调用,零查询开销。 */
export function useQQFaceLookup(): (id: string) => QQSystemFace | undefined {
    return useCallback((id: string) => qqFaceService.peek(id), []);
}

/** 最近使用:本地存储读取 + 会话内覆盖,与选择器 recent state 语义一致。 */
export function useRecentQQFaces(identity: string): {
    ids: string[];
    remember: (id: string) => void;
} {
    const [recent, setRecent] = useState<{ identity: string; ids: string[] } | null>(null);
    const saved = useMemo(() => loadRecentQQFaces(identity), [identity]);
    const ids = recent?.identity === identity ? recent.ids : saved;
    const remember = useCallback(
        (id: string) => setRecent({ identity, ids: rememberQQFace(identity, id) }),
        [identity],
    );
    return { ids, remember };
}

export const acquireQQFaceAsset = (source: string): Promise<QQFaceImageLease> =>
    qqFaceAssetService.acquire(source);

export const invalidateQQFaceAsset = (source: string): Promise<void> =>
    qqFaceAssetService.invalidate(source);
