// OneBot 调试台 IPC。对接 src-tauri/src/commands/onebot_debug.rs；浏览器预览走 mock 里的三个假 Bot。
//
// 命令只在这里出现。参数键按 Tauri 的约定写 camelCase（Rust 侧是 snake_case）。
// 调用失败分两种：没拿到回包的原因（超时、通道不可用……）放在 `DebugCallResponse.result` 里当数据返回，
// 不会抛；其余命令失败时 invoke 抛出后端给的中文字符串，由调用方经 `errorText` 处理。

import { Channel, invoke, isTauri, pickTextFiles, saveFileAs } from '../ipc/transport';
import { onebotDebugMock } from '../ipc/mock/onebot-debug.mock';
import type { BackendType } from '../ipc/generated/domain/BackendType';
import type { DebugActionSpec } from '../ipc/generated/debug/DebugActionSpec';
import type { DebugCallRequest } from '../ipc/generated/debug/DebugCallRequest';
import type { DebugCallResponse } from '../ipc/generated/debug/DebugCallResponse';
import type { DebugCatalog } from '../ipc/generated/debug/DebugCatalog';
import type { DebugChannelId } from '../ipc/generated/debug/DebugChannelId';
import type { DebugChannelInfo } from '../ipc/generated/debug/DebugChannelInfo';
import type { DebugChannels } from '../ipc/generated/debug/DebugChannels';
import type { DebugCollections } from '../ipc/generated/debug/DebugCollections';
import type { DebugEvent } from '../ipc/generated/debug/DebugEvent';
import type { DebugEventBatch } from '../ipc/generated/debug/DebugEventBatch';
import type { DebugHistoryEntry } from '../ipc/generated/debug/DebugHistoryEntry';
import type { DebugHistoryPage } from '../ipc/generated/debug/DebugHistoryPage';
import type { DebugHistoryQuery } from '../ipc/generated/debug/DebugHistoryQuery';
import type { DebugReceiverInfo } from '../ipc/generated/debug/DebugReceiverInfo';
import type { DebugStorageNotice } from '../ipc/generated/debug/DebugStorageNotice';
import type { DebugSubscribeResponse } from '../ipc/generated/debug/DebugSubscribeResponse';
import type { DebugTarget } from '../ipc/generated/debug/DebugTarget';
import type { DebugWorkspace } from '../ipc/generated/debug/DebugWorkspace';

// 浏览器预览里导出 / 导入共用的假路径（mock 把导出的内容存在内存里，按路径取回）
const PREVIEW_COLLECTIONS_PATH = 'preview://onebot-debug-collections.json';

export const onebotDebugService = {
    /** 左栏 / 顶栏的 Bot 列表 */
    targets: (): Promise<DebugTarget[]> =>
        isTauri ? invoke('onebot_debug_targets') : onebotDebugMock.targets(),

    /** 一个 Bot 的全部通道和「自动」当前会落到哪条 */
    channels: (botId: string): Promise<DebugChannels> =>
        isTauri ? invoke('onebot_debug_channels', { botId }) : onebotDebugMock.channels(botId),

    /** 连通测试；内部通道探调试接口在不在，真实通道调一次只读接口 */
    testChannel: (botId: string, channel: DebugChannelId): Promise<DebugChannelInfo> =>
        isTauri
            ? invoke('onebot_debug_test_channel', { botId, channel })
            : onebotDebugMock.testChannel(botId, channel),

    /** botId 为空时只用内置快照 */
    catalog: (botId: string | null, backend: BackendType): Promise<DebugCatalog> =>
        isTauri
            ? invoke('onebot_debug_catalog', { botId, backend })
            : onebotDebugMock.catalog(botId, backend),

    describe: (
        botId: string | null,
        backend: BackendType,
        action: string,
    ): Promise<DebugActionSpec | null> =>
        isTauri
            ? invoke('onebot_debug_describe', { botId, backend, action })
            : onebotDebugMock.describe(botId, backend, action),

    call: (request: DebugCallRequest): Promise<DebugCallResponse> =>
        isTauri ? invoke('onebot_debug_call', { request }) : onebotDebugMock.call(request),

    /** 只是不再等待，不保证上游没执行 */
    cancel: (requestId: string): Promise<void> =>
        isTauri ? invoke('onebot_debug_cancel', { requestId }) : onebotDebugMock.cancel(requestId),

    /** 把被截断的回包完整另存到 path（路径由系统「另存为」对话框给） */
    saveResponse: (requestId: string, path: string): Promise<void> =>
        isTauri
            ? invoke('onebot_debug_save_response', { requestId, path })
            : onebotDebugMock.saveResponse(requestId, path),

    /**
     * 「另存完整内容」：先弹系统的另存为对话框，再让后端把这次被截断的回包全文写过去。
     * 用户取消了返回 false。后端只留最近几次被截断的回包，太早的会失败（抛后端给的原因）。
     * 浏览器预览没有系统对话框：写进 mock 的内存文件，整条路照样能走通。
     */
    saveResponseFile: async (requestId: string, defaultName: string): Promise<boolean> => {
        if (!isTauri) {
            await onebotDebugMock.saveResponse(requestId, `preview://${defaultName}`);
            return true;
        }
        const path = await saveFileAs('另存完整回包', defaultName, [{ name: 'JSON', extensions: ['json'] }]);
        if (!path) return false;
        await invoke('onebot_debug_save_response', { requestId, path });
        return true;
    },

    /** 开始收一个 Bot 的事件：先补缓冲里已有的，再推实时的（每约 50ms 一批） */
    subscribe: (
        botId: string,
        source: DebugChannelId,
        onBatch: (batch: DebugEventBatch) => void,
    ): Promise<DebugSubscribeResponse> => {
        if (!isTauri) return onebotDebugMock.subscribe(botId, source, onBatch);
        const events = new Channel<DebugEventBatch>();
        events.onmessage = (batch) => onBatch(batch);
        return invoke('onebot_debug_subscribe', { botId, source, events });
    },

    /** 只是不再看；后端的接收器继续收，直到空闲超时或被停 */
    unsubscribe: (subscriptionId: string): Promise<void> =>
        isTauri
            ? invoke('onebot_debug_unsubscribe', { subscriptionId })
            : onebotDebugMock.unsubscribe(subscriptionId),

    receivers: (): Promise<DebugReceiverInfo[]> =>
        isTauri ? invoke('onebot_debug_receivers') : onebotDebugMock.receivers(),

    stopReceiver: (botId: string): Promise<void> =>
        isTauri ? invoke('onebot_debug_stop_receiver', { botId }) : onebotDebugMock.stopReceiver(botId),

    /** 按序号游标读缓冲：seq 大于 sinceSeq 的前 limit 条，按 seq 升序 */
    readEvents: (botId: string, sinceSeq: number, limit: number): Promise<DebugEvent[]> =>
        isTauri
            ? invoke('onebot_debug_read_events', { botId, sinceSeq, limit })
            : onebotDebugMock.readEvents(botId, sinceSeq, limit),

    workspace: (): Promise<DebugWorkspace> =>
        isTauri ? invoke('onebot_debug_workspace') : onebotDebugMock.workspace(),

    saveWorkspace: (workspace: DebugWorkspace): Promise<void> =>
        isTauri
            ? invoke('onebot_debug_save_workspace', { workspace })
            : onebotDebugMock.saveWorkspace(workspace),

    collections: (): Promise<DebugCollections> =>
        isTauri ? invoke('onebot_debug_collections') : onebotDebugMock.collections(),

    saveCollections: (collections: DebugCollections): Promise<void> =>
        isTauri
            ? invoke('onebot_debug_save_collections', { collections })
            : onebotDebugMock.saveCollections(collections),

    exportCollections: (path: string): Promise<void> =>
        isTauri
            ? invoke('onebot_debug_export_collections', { path })
            : onebotDebugMock.exportCollections(path),

    importCollections: (path: string): Promise<DebugCollections> =>
        isTauri
            ? invoke('onebot_debug_import_collections', { path })
            : onebotDebugMock.importCollections(path),

    /**
     * 「导出收藏」的另存为对话框；取消返回 null。
     * 浏览器预览没有系统对话框：给一个固定的假路径，导出和导入都落在 mock 的内存文件里，能走通一整圈。
     */
    saveCollectionsFile: (defaultName = 'onebot-debug-collections.json'): Promise<string | null> =>
        isTauri
            ? saveFileAs('导出调试台收藏', defaultName, [{ name: 'JSON', extensions: ['json'] }])
            : Promise.resolve(PREVIEW_COLLECTIONS_PATH),

    /**
     * 「导入收藏」的选文件对话框，只取第一个；取消返回 null。
     * 系统对话框是多选的，用户多选了就只导第一个，`ignored` 是被略过的个数，由调用方告诉用户。
     */
    pickCollectionsFile: async (): Promise<{ path: string; ignored: number } | null> => {
        if (!isTauri) return { path: PREVIEW_COLLECTIONS_PATH, ignored: 0 };
        const paths = await pickTextFiles('选择要导入的收藏文件');
        return paths.length > 0 ? { path: paths[0], ignored: paths.length - 1 } : null;
    },

    history: (query: DebugHistoryQuery): Promise<DebugHistoryPage> =>
        isTauri ? invoke('onebot_debug_history', { query }) : onebotDebugMock.history(query),

    historyEntry: (id: string): Promise<DebugHistoryEntry | null> =>
        isTauri ? invoke('onebot_debug_history_entry', { id }) : onebotDebugMock.historyEntry(id),

    clearHistory: (): Promise<void> =>
        isTauri ? invoke('onebot_debug_clear_history') : onebotDebugMock.clearHistory(),

    /** 落盘文件损坏被挪走时的提示 */
    storageNotices: (): Promise<DebugStorageNotice[]> =>
        isTauri ? invoke('onebot_debug_storage_notices') : onebotDebugMock.storageNotices(),
};
