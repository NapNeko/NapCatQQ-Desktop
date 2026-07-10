// ncd-watch 配置同步 IPC。
// 命令字面量只在此文件出现（R3）。

import { invoke, isTauri } from '../ipc/transport';

export const ncdWatchService = {
    /** 将该 server 上的 Bot 列表 + 已保存 Webhook 写入远端 notify.json，并刷 present */
    syncNotify: async (serverId: string): Promise<void> => {
        if (!isTauri) {
            throw new Error('浏览器预览不支持同步 ncd-watch');
        }
        await invoke<void>('sync_ncd_watch_notify', { serverId });
    },

    /** 仅刷新 desktop_present 心跳 */
    touchPresent: async (serverId: string): Promise<void> => {
        if (!isTauri) {
            throw new Error('浏览器预览不支持 ncd-watch 心跳');
        }
        await invoke<void>('touch_ncd_watch_present', { serverId });
    },
};
