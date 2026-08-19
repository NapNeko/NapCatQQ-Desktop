// 桌面端展示用元信息（与 package.json / tauri.conf.json 发布版本对齐）。

/** 用户可见的应用版本号（带 v 前缀）。 */
export const APP_VERSION_LABEL = 'v3.1.8';

/** 裸语义版本，供比对或 IPC 需要时使用。 */
export const APP_VERSION = '3.1.8';

/** 产品显示名（侧栏 / 关于 / 启动层一致）。 */
export const APP_PRODUCT_NAME = 'NapCatQQ Desktop';

/** 开源许可证 SPDX 标识。 */
export const APP_LICENSE_SPDX = 'GPL-3.0';

/** 上游 GitHub 仓库（owner/repo）。 */
export const APP_GITHUB_REPO = 'NapNeko/NapCatQQ-Desktop';

/** 仓库主页。 */
export const APP_GITHUB_URL = `https://github.com/${APP_GITHUB_REPO}`;

/** Releases 列表页。 */
export const APP_RELEASES_URL = `${APP_GITHUB_URL}/releases`;

/** LICENSE 原文（GitHub blob，默认分支 main）。 */
export const APP_LICENSE_URL = `${APP_GITHUB_URL}/blob/main/LICENSE`;
