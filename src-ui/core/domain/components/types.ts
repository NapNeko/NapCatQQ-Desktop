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

// ─── 主机主导视图（机器 × 各组件）─────────────────────────────────────
//
// 把"组件 × 各主机"矩阵翻成"主机 × 各组件"：一台机器一张卡，卡内逐组件一行。
// 不支持当前平台的组件直接从该机器卡里剔掉（本机 Windows 不列 Linux-only 的
// noVNC / Node.js）。

/// 机器卡里的一行：组件元数据 + 该组件在这台机器上的状态。
export interface MachineComponentRow {
    info: ComponentInfo;
    status: HostComponentStatus;
}

/// 一台机器卡所需的数据：主机信息 + 该机器上每个组件的状态行（按 category 分组）。
export interface MachineView {
    host: HostInfo;
    framework: MachineComponentRow[];
    runtimeDep: MachineComponentRow[];
    selfApp: MachineComponentRow[];
}

/// 把 ComponentRow[]（组件主导）翻成 MachineView[]（主机主导）。
/// 顺序：本机优先，远端按 hosts 输入顺序。组件在每台机器内按 category 分组，
/// unsupported 的行直接剔掉（机器卡不展示这台装不了的东西）。
export function groupByHost(
    rows: ComponentRow[],
    hosts: HostInfo[],
): MachineView[] {
    return hosts.map((host) => {
        const machine: MachineView = {
            host,
            framework: [],
            runtimeDep: [],
            selfApp: [],
        };
        for (const compRow of rows) {
            const statusRow = compRow.rows.find((r) => r.host.host_id === host.host_id);
            if (!statusRow) continue;
            // 这台机器装不了的组件不进卡。
            if (statusRow.status.state === 'unsupported') continue;
            const row: MachineComponentRow = {
                info: compRow.info,
                status: statusRow.status,
            };
            switch (compRow.info.category) {
                case 'framework':
                    machine.framework.push(row);
                    break;
                case 'runtime_dep':
                    machine.runtimeDep.push(row);
                    break;
                case 'self_app':
                    machine.selfApp.push(row);
                    break;
            }
        }
        return machine;
    });
}

/// 一台机器卡上所有可装组件里，已装数 / 总数。给主机切换条上的小计数徽章用，
/// 让用户不展开就能比较"哪台机器装得多"。total 已排除 unsupported（groupByHost
/// 早就把装不了的剔掉了）。
export function machineSummary(machine: MachineView): { installed: number; total: number } {
    const all = [...machine.framework, ...machine.runtimeDep, ...machine.selfApp];
    const installed = all.filter((r) => r.status.state === 'installed').length;
    return { installed, total: all.length };
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
        return { state: 'unknown', reason: '正在探测' };
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

// ─── 主机连接失败判断 ─────────────────────────────────────────────────
// 探测 reason 如果包含 SSH 超时 / 连接拒绝 / 自动连接失败等关键词，
// 视为整机连通性问题（而非单个组件探测逻辑失败）。

const CONNECTIVITY_PATTERNS = [
    '连接失败',
    '自动连接失败',
    'ssh connect failed',
    'os error 10060',
    'connection timed out',
    'connection refused',
    '没有反应',
    '不可达',
    'network is unreachable',
    'no route to host',
];

export function isHostConnectivityFailureReason(reason: string | undefined | null): boolean {
    if (!reason) return false;
    const lower = reason.toLowerCase();
    return CONNECTIVITY_PATTERNS.some((p) => lower.includes(p));
}
