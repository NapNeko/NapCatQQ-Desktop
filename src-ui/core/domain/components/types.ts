// Components 页 UI view 类型。
//
// 后端只给两类原始数据：
//   1. ComponentInfo[]  —— 静态元数据（每个组件的名字 / 仓库 / 支持平台 / 分类）
//   2. ComponentDetectResult  —— 单组件 × 单主机的探测结果
//
// 前端要展示的是"每个组件 × 所有已知主机"的二维矩阵。Domain 层负责把
// 上面两类数据合成为渲染友好的 ComponentRow 列表，UI 直接 map 渲染。

import type {
    ComponentCategory,
    ComponentDetectResult,
    ComponentId,
    ComponentInfo,
    DetectedVersion,
    Locality,
    Os,
    SupportedTarget,
} from '../../ipc/types';

/// 主机简化视图，前端只关心 host_id + 显示名 + 平台属性。
export interface HostInfo {
    host_id: string;
    display_name: string;
    os: Os;
    locality: Locality;
}

/// 单主机上某组件的状态。
export type HostComponentStatus =
    /// 当前 host 不在 Component 的 supported_targets 内（例如 SnowLuma 装本机 macOS）
    | { state: 'unsupported' }
    /// 支持但未安装
    | { state: 'not_installed' }
    /// 已安装；如果有远端 release，可外部派生 hasUpdate
    | { state: 'installed'; detected: DetectedVersion }
    /// 探测失败 / 主机断连等异常
    | { state: 'unknown'; reason: string };

/// 一行：某主机上某组件的当前状态 + 派生属性。
export interface HostStatusRow {
    component_id: ComponentId;
    host: HostInfo;
    status: HostComponentStatus;
}

/// 一个组件卡片所需的全部数据：元数据 + 各主机的状态行。
export interface ComponentRow {
    info: ComponentInfo;
    /// 每台已知主机一行；顺序由 hosts 输入决定。
    rows: HostStatusRow[];
}

/// 整个页面的 view model。
export interface ComponentsView {
    framework: ComponentRow[];
    runtimeDep: ComponentRow[];
    selfApp: ComponentRow[];
}

// 内部辅助：判断 host 是否在 component 的 supported_targets 内。
export function hostSupportsComponent(
    host: HostInfo,
    targets: SupportedTarget[],
): boolean {
    return targets.some((t) => t.os === host.os && t.locality === host.locality);
}

// 把 detect 结果映射成 HostComponentStatus。
export function deriveStatus(
    host: HostInfo,
    info: ComponentInfo,
    detect: ComponentDetectResult | null,
): HostComponentStatus {
    if (!hostSupportsComponent(host, info.supported_targets)) {
        return { state: 'unsupported' };
    }
    if (!detect) {
        return { state: 'unknown', reason: '尚未探测' };
    }
    if (!detect.supported) {
        return { state: 'unsupported' };
    }
    if (detect.detected) {
        return { state: 'installed', detected: detect.detected };
    }
    return { state: 'not_installed' };
}

// 按分类把 ComponentRow[] 切成 ComponentsView。
export function splitByCategory(rows: ComponentRow[]): ComponentsView {
    const out: ComponentsView = { framework: [], runtimeDep: [], selfApp: [] };
    for (const row of rows) {
        switch (row.info.category) {
            case 'framework':
                out.framework.push(row);
                break;
            case 'runtime_dep':
                out.runtimeDep.push(row);
                break;
            case 'self_app':
                out.selfApp.push(row);
                break;
        }
    }
    return out;
}

// 字面量工具
const CATEGORY_LABEL: Record<ComponentCategory, string> = {
    framework: '框架',
    runtime_dep: '运行时依赖',
    self_app: '桌面端自身',
};

export function categoryLabel(c: ComponentCategory): string {
    return CATEGORY_LABEL[c];
}
