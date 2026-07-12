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
        tag: 'v4.20.0',
        published_at: BigInt(NOW - 86400 * 3),
        html_url: 'https://github.com/NapNeko/NapCatQQ/releases/tag/v4.20.0',
        release_notes: [
            '## 修复',
            '',
            '- 修复部分场景下登录状态抖动',
            '- 提升 WebUI 稳定性',
            '',
            '## 改进',
            '',
            '- 优化消息转发链路延迟',
        ].join('\n'),
        assets: [
            {
                name: 'NapCat.Shell.zip',
                sha256: '0'.repeat(64),
            },
        ],
    },
    snowluma_latest: {
        version: '1.9.2',
        tag: 'v1.9.2',
        published_at: BigInt(NOW - 86400 * 5),
        html_url: 'https://github.com/SnowLuma/SnowLuma/releases/tag/v1.9.2',
        release_notes: [
            '## SnowLuma v1.9.2',
            '',
            '- 修复 daemon 重连后状态不同步',
            '- 改进协议同意流程文案',
            '',
            '详见上游 Release。',
        ].join('\n'),
        assets: [],
    },
    desktop_latest: {
        version: '3.0.0',
        tag: 'v3.0.0',
        published_at: BigInt(NOW - 86400),
        html_url: 'https://github.com/NapNeko/NapCatQQ-Desktop/releases/tag/v3.0.0',
        release_notes: [
            '## NapCatQQ Desktop v3.0.0',
            '',
            '### 新架构',
            '',
            '- Rust + Tauri + React 主线',
            '- 组件页支持查看 SL / NC / NCD 更新日志',
            '',
            '### 兼容',
            '',
            '- 保留 ProgramData 数据根与旧配置迁移路径',
        ].join('\n'),
        assets: [],
    },
    ncd_watch_latest: {
        version: '0.2.0',
        tag: 'watch-v0.2.0',
        published_at: BigInt(NOW - 86400 * 2),
        html_url:
            'https://github.com/NapNeko/NapCatQQ-Desktop/releases/tag/watch-v0.2.0',
        release_notes: [
            '## ncd-watch 0.2.0',
            '',
            '- 远端主机侧探活与 Webhook 同步',
            '- 登录态分层探测',
        ].join('\n'),
        assets: [],
    },
    qq_linux_latest: {
        version: '3.2.31',
        tag: 'pcConfig',
        published_at: BigInt(NOW - 86400),
        html_url: 'https://im.qq.com/',
        release_notes: 'Linux QQ via pcConfig（无 GitHub body，仅版本探测）',
        assets: [],
    },
    qq_windows_latest: {
        version: '9.9.31',
        tag: 'pcConfig',
        published_at: BigInt(NOW - 86400),
        html_url: 'https://im.qq.com/',
        release_notes: 'Windows QQ via pcConfig（无 GitHub body，仅版本探测）',
        assets: [],
    },
    fetched_at: BigInt(NOW),
};
