// 应用实例列表在 react-query 里的缓存键和就地替换。
// 单独成文件：根组件的事件桥要用，不能为此把应用端页的整套 hook 拖进主包。

import type { AppInstance } from '../../core/ipc/types';

export const APP_INSTANCES_KEY = ['appInstances'] as const;

export function upsertInstance(list: AppInstance[] | undefined, next: AppInstance): AppInstance[] {
    if (!list?.length) return [next];
    const idx = list.findIndex((i) => i.id === next.id);
    if (idx < 0) return [...list, next];
    return list.map((i) => (i.id === next.id ? next : i));
}
