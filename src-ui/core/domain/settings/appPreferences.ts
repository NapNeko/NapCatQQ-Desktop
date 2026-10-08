// 客户端 UI 偏好形状与规范化：core/domain 权威定义。
// preferencesStore（hooks 层）只负责 localStorage 落盘 + DOM 副作用 + React 订阅，
// 形状与 normalize 规则以本文件为准，供 core/services、core/domain 同层引用。

import { type MotionLevel } from '../../design/motion';
import { type RadiusStyle } from '../../design/radius';
import type { ThemeMode } from '../../design/themes/registry';

export type CloseAction = 'close' | 'tray';

export function normalizeCloseAction(raw: unknown): CloseAction {
    return raw === 'tray' ? 'tray' : 'close';
}

export interface AppPreferences {
    theme: ThemeMode;
    showMascot: boolean;
    closeAction: CloseAction;
    motionEnabled: boolean;
    motionLevel: MotionLevel;
    motionSpeed: number;
    radiusStyle: RadiusStyle;
    sidebarStyle: SidebarStyle;
}

export type SidebarStyle = 'classic' | 'floating';

// 缺省/非法值都回落悬浮：老配置覆盖更新后默认新形态，设置里可切回经典。
export function normalizeSidebarStyle(raw: unknown): SidebarStyle {
    return raw === 'classic' ? 'classic' : 'floating';
}
