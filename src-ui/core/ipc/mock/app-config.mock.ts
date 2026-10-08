// 浏览器预览模式下的应用端实例配置假数据入口：内存版类型化配置 + 版本号递增 + 冲突模拟。
// 按框架拆在 ./app-config/ 目录（data/texts/shared 公共件，每框架一文件，api.ts 做分发）。
//
// 冲突模拟：控制台执行 `__ncdMock.appConfigConflictOnce()` 后，下一次带 base_revision 的保存
// 会返回 conflict（等价于「Karin WebUI 在你编辑期间改了文件」）。

export { editKoishiConfig, peekKoishiConfig } from './app-config/koishi';
export { peekMaiBotConfig, syncMaiBotLink } from './app-config/maibot';
export { syncYunzaiLink } from './app-config/yunzai';
export { peekKarinHttpAuthKey, syncKarinLinkToken } from './app-config/karin';
export { createMockAppConfigApi, mockAppConfigControls } from './app-config/api';
export type { MockAppConfigDeps } from './app-config/shared';

import { mockAppConfigControls } from './app-config/api';

if (typeof window !== 'undefined') {
    const w = window as unknown as { __ncdMock?: Record<string, unknown> };
    w.__ncdMock = { ...(w.__ncdMock ?? {}), ...mockAppConfigControls };
}
