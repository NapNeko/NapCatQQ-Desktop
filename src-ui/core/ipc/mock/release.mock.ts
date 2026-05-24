// 浏览器预览模式下的 ReleaseSnapshot 假数据库。
// 真 IPC 实装在 `core/services/release.service.ts`。
//
// 注意：`published_at` / `fetched_at` 是 bigint（ts-rs 派生 u64 的产物）。
// 这一层只负责"假数据 + 形状"，bigint → number 转换由 domain 层做。

import type { ReleaseSnapshot } from '../types';

const NOW = Math.floor(Date.now() / 1000);

export const mockReleaseSnapshot: ReleaseSnapshot = {
    napcat_latest: {
        version: '4.20.0',
        published_at: BigInt(NOW - 86400 * 3),
        html_url: 'https://github.com/NapNeko/NapCatQQ/releases/tag/v4.20.0',
        release_notes: '修复一些已知问题，提升登录稳定性。',
    },
    snowluma_latest: null,
    desktop_latest: null,
    fetched_at: BigInt(NOW),
};
