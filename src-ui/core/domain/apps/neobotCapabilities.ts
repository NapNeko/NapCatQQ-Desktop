// NeoBot 各能力要求的**最低版本**声明。
//
// 为什么要有这一层：面板接口是逐步加的。拿 1.2.3 的实例打开「部署」页，桌面端会拿到 404，
// 屏幕上就是一句「面板返回 404 Not Found」——用户看不懂，也不知道该做什么（实测反馈）。
// 把「哪个能力从哪版起有」集中声明在这里，页面就能**提前**说清楚，而不是等接口报错。
//
// 版本号取的是 NeoBot 的对外版本（pyproject / 面板 version 字段）。

import { compareAppVersion } from './appVersions';

export const NEOBOT_MIN_VERSION = {
    /**
     * 面板登录：密码换会话 token（X-Token / X-CSRF-Token）那一套。
     * 更老的版本没有这套接口，桌面端**代不了登录**，只能提示用户去升级或直接用面板。
     */
    panelLogin: '1.2.4a1',
    /** 快捷部署菜单：/api/deploy/status 与 /api/deploy/onebot-token */
    deployApi: '1.2.4a1',
    /** 优雅关闭：/api/admin/shutdown（桌面端停止实例时先请它自己退） */
    gracefulStop: '1.2.3',
} as const;

export type NeoBotCapability = keyof typeof NEOBOT_MIN_VERSION;

/**
 * 装了的版本够不够某个能力。
 *
 * 版本未知时返回 true：宁可让请求打过去、由面板自己回答，也别因为读不到版本就把
 * 本来能用的页面挡掉。
 *
 * 注意 compareAppVersion 的方向：它返回 >0 表示 installed 比 min **旧**
 * （见 core/domain/apps/appVersions.ts 的文档），所以这里是 <= 0 才算够。
 */
export function meetsNeoBotVersion(
    installed: string | null | undefined,
    capability: NeoBotCapability,
): boolean {
    const min: string = NEOBOT_MIN_VERSION[capability];
    if (!installed || !installed.trim()) return true;
    return compareAppVersion(installed, min) <= 0;
}

/** 给提示用的一句话，例如「需要 NeoBot 1.2.4a1 及以上，当前 1.2.3」 */
export function versionRequirementText(
    installed: string | null | undefined,
    capability: NeoBotCapability,
): string {
    const min: string = NEOBOT_MIN_VERSION[capability];
    const now = installed && installed.trim() ? installed : '未知';
    return '需要 NeoBot ' + min + ' 及以上，当前 ' + now;
}
