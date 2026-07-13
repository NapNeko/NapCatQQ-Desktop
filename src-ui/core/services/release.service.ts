// 远端 release 快照 IPC 服务。
// 唯一持有 `get_release_snapshot` 命令名字符串的位置（R3：单一字面量来源）。
//
// 后端实装在 `crates/ncd-runtime/src/release.rs` + `src-tauri/src/commands/release.rs`，
// 默认 1 小时 TTL 复用本地缓存；force=true 跳过磁盘 TTL（用户点刷新）。
// 超时 / 网络错误一律返回字段为 None 的 ReleaseSnapshot（后端永远不向前端抛错）。

import { invoke, isTauri } from '../ipc/transport';
import type { ReleaseSnapshot } from '../ipc/types';
import { mockReleaseSnapshot } from '../ipc/mock/release.mock';
import { withMockDelay } from '../ipc/mock/bootstrap.mock';

export const releaseService = {
    /** force: 跳过后端磁盘 TTL，强制走中转/GitHub */
    getSnapshot: async (force = false): Promise<ReleaseSnapshot> => {
        if (isTauri) {
            return invoke<ReleaseSnapshot>('get_release_snapshot', { force });
        }
        return withMockDelay(mockReleaseSnapshot, 250);
    },
};
