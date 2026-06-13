// Lucide + MotionIcon 语义映射：按场景选 preset，避免各页面散落魔法字符串。

import type { MotionIconPreset } from './MotionIcon';

type NavRouteId =
    | 'overview'
    | 'bots'
    | 'components'
    | 'docker'
    | 'remote'
    | 'tasks'
    | 'settings';

/** 侧栏当前路由持续动效（与 Sidebar NAV_ACTIVE_MOTION 一致）。 */
export const NAV_ROUTE_MOTION: Record<NavRouteId, MotionIconPreset> = {
    overview: 'bob',
    bots: 'pulse',
    components: 'nudge',
    docker: 'breathe',
    remote: 'breathe',
    tasks: 'nudge',
    settings: 'spin-slow',
};

/** 分段控件选中项：轻呼吸 + 切换时描边进场。 */
export function segmentMotion(selected: boolean): MotionIconPreset {
    return selected ? 'breathe' : 'none';
}

/** 刷新 / 加载：忙时旋转，闲时静止。 */
export function refreshMotion(busy: boolean): MotionIconPreset {
    return busy ? 'spin' : 'none';
}

/** 工具栏主操作（新增、上传）：选中或强调时用轻弹。 */
export const EMPHASIS_MOTION: MotionIconPreset = 'nudge';

/** 运行中 / 在线：缓慢脉冲。 */
export const LIVE_MOTION: MotionIconPreset = 'pulse';

/** 容器 / 远端等「后台资源」：呼吸。 */
export const RESOURCE_MOTION: MotionIconPreset = 'breathe';

/** 配置 / 设置齿轮：慢转（仅 active 态用）。 */
export const SETTINGS_MOTION: MotionIconPreset = 'spin-slow';

/** FAB 主按钮（新增 Bot）：进场后轻微 pulse。 */
export const FAB_PRIMARY_MOTION: MotionIconPreset = 'pulse';

/** 批量 / 列表模式入口：轻推。 */
export const BATCH_MOTION: MotionIconPreset = 'nudge';

/** InfoBar tone 对应左侧图标（进场由 InfoBarStack 管，静态可挂轻动效）。 */
export function infoToneMotion(
    tone: 'info' | 'success' | 'warning' | 'danger',
): MotionIconPreset {
    switch (tone) {
        case 'danger':
            return 'wiggle';
        case 'warning':
            return 'nudge';
        case 'success':
            return 'pulse';
        case 'info':
        default:
            return 'none';
    }
}