// 远端 release 快照 IPC 服务。
// 唯一持有 `get_release_snapshot` 命令名字符串的位置（R3：单一字面量来源）。
//
// 后端实装在 `crates/ncd-runtime/src/release.rs` + `src-tauri/src/commands/release.rs`，
// 1 小时 TTL 内复用本地缓存，超时 / 网络错误一律返回字段为 None 的 ReleaseSnapshot
// （后端永远不向前端抛错），所以本服务只走 ok 路径。

import { invoke, isTauri } from '../ipc/transport';
import type { ReleaseSnapshot } from '../ipc/types';
import { mockReleaseSnapshot } from '../ipc/mock/release.mock';
import { withMockDelay } from '../ipc/mock/bootstrap.mock';

export const releaseService = {
    getSnapshot: async (): Promise<ReleaseSnapshot> => {
        if (isTauri) return invoke<ReleaseSnapshot>('get_release_snapshot');
        return withMockDelay(mockReleaseSnapshot, 250);
    },
};
