// 可选功能模块开关（设置 · 功能）。落在 app-settings.json 的 features，默认全开。
//
// 只收能整块拿掉、不牵连主路径（装、配、启停 Bot）的模块。新增一项：Rust 端
// `FeatureToggles` 加字段 → 这里 FEATURE_GROUPS 加一条（关不得的情况写进 featureOffBlock）
// → 在各入口用 useFeatureEnabled 判断。

import type { FeatureToggles } from '../../ipc/generated/domain/FeatureToggles';

export type { FeatureToggles };
/** 布尔开关；应用端框架名单另算 */
export type FeatureKey = Exclude<keyof FeatureToggles, 'hiddenAppFrameworks'>;

export const DEFAULT_FEATURES: FeatureToggles = {
    napcat: true,
    snowluma: true,
    apps: true,
    hiddenAppFrameworks: [],
    dockerPage: true,
    ncdWatch: true,
    terminal: true,
    apiDebug: true,
};

export interface FeatureDef {
    key: FeatureKey;
    label: string;
    /** 关掉后界面上少掉哪些东西 */
    description: string;
    /** 关掉后后台少干哪些活；只是藏入口、不省资源的就不写 */
    saves?: string;
}

export interface FeatureGroup {
    title: string;
    items: FeatureDef[];
}

export const FEATURE_GROUPS: ReadonlyArray<FeatureGroup> = [
    {
        title: '协议端',
        items: [
            {
                key: 'napcat',
                label: 'NapCat',
                description: '组件页的 NapCat 行、新建 Bot 时的 NapCat 选项',
                saves: '启动时不再在每台主机上探测 NapCat',
            },
            {
                key: 'snowluma',
                label: 'SnowLuma',
                description: '组件页的 SnowLuma 行、新建 Bot 时的 SnowLuma 选项',
                saves: '启动时不再在每台主机上探测 SnowLuma',
            },
        ],
    },
    {
        title: '应用端',
        items: [
            {
                key: 'apps',
                label: '应用端',
                description:
                    '应用端页、组件页的应用端组、Bot 配置里的「对接应用端」。下面可以只藏其中几个框架',
                saves: '实例不再随桌面端启动（每个实例是一个常驻的 Node / Python 进程）',
            },
        ],
    },
    {
        title: '远端主机',
        items: [
            {
                key: 'dockerPage',
                label: '容器页',
                description: '管理远端主机上的 Docker 和容器。开着时也只在有远端主机 Docker 可用时出现',
                saves: '启动时不再到每台远端主机上探测 Docker',
            },
            {
                key: 'ncdWatch',
                label: '远端值守（ncd-watch）',
                description: '桌面端退出后由远端主机继续盯 Bot 掉线。关掉后隐藏组件页的 ncd-watch 行和「设置 · 通知」里的远端值守',
                saves: '不再每 45 秒给远端主机写心跳，也不再探测 ncd-watch',
            },
        ],
    },
    {
        title: '工具',
        items: [
            {
                key: 'terminal',
                label: '内嵌终端',
                description: '标题栏的终端按钮、Ctrl+` 快捷键、各卡片上的终端入口、「设置 · 终端」',
                saves: '终端面板和 xterm（约 570 KB 脚本）不再加载',
            },
            {
                key: 'apiDebug',
                label: 'OneBot 调试台',
                description: '侧栏和 Bot 卡片的「调试」入口',
                saves: '调试台的接收器、会话和在途调用全部停掉（连 MCP 服务也不能再调）',
            },
        ],
    },
];

export const FEATURE_DEFS: ReadonlyArray<FeatureDef> = FEATURE_GROUPS.flatMap((g) => g.items);

const BOOL_KEYS: ReadonlyArray<FeatureKey> = ['napcat', 'snowluma', 'apps', 'dockerPage', 'ncdWatch', 'terminal', 'apiDebug'];

/** 磁盘上缺字段或不是布尔值的一律当开着；两个协议端都关了就开回 NapCat（和 Rust 端一致）。 */
export function normalizeFeatures(
    raw: Partial<Record<keyof FeatureToggles, unknown>> | null | undefined,
): FeatureToggles {
    const out = { ...DEFAULT_FEATURES };
    for (const key of BOOL_KEYS) out[key] = raw?.[key] !== false;
    if (!out.napcat && !out.snowluma) out.napcat = true;
    const hidden = Array.isArray(raw?.hiddenAppFrameworks) ? raw.hiddenAppFrameworks : [];
    out.hiddenAppFrameworks = [...new Set(hidden.filter((id): id is string => typeof id === 'string' && id !== ''))];
    return out;
}

export function featuresEqual(a: FeatureToggles, b: FeatureToggles): boolean {
    return (
        BOOL_KEYS.every((key) => a[key] === b[key]) &&
        a.hiddenAppFrameworks.length === b.hiddenAppFrameworks.length &&
        a.hiddenAppFrameworks.every((id) => b.hiddenAppFrameworks.includes(id))
    );
}

export function isAppFrameworkVisible(features: FeatureToggles, frameworkId: string): boolean {
    return features.apps && !features.hiddenAppFrameworks.includes(frameworkId);
}

export function setAppFrameworkVisible(
    features: FeatureToggles,
    frameworkId: string,
    visible: boolean,
): FeatureToggles {
    const rest = features.hiddenAppFrameworks.filter((id) => id !== frameworkId);
    return { ...features, hiddenAppFrameworks: visible ? rest : [...rest, frameworkId] };
}

/** 组件目录里哪些组件跟着开关藏（组件页不列、启动时也不探测）。 */
export function isComponentHiddenByFeatures(features: FeatureToggles, componentId: string): boolean {
    switch (componentId) {
        case 'napcat':
            return !features.napcat;
        case 'snowluma':
            return !features.snowluma;
        case 'ncd_watch':
            return !features.ncdWatch;
        default:
            return false;
    }
}

/** 判断能不能关要用到的现状。拿不到的数（还在加载）按 0 算。 */
export interface FeatureUsage {
    botsByBackend: { napcat: number; snowluma: number };
    activeAppInstances: number;
    instancesByFramework: Readonly<Record<string, number>>;
    hostsWithNcdWatch: number;
    openTerminals: number;
}

export const EMPTY_FEATURE_USAGE: FeatureUsage = {
    botsByBackend: { napcat: 0, snowluma: 0 },
    activeAppInstances: 0,
    instancesByFramework: {},
    hostsWithNcdWatch: 0,
    openTerminals: 0,
};

/**
 * 现在关掉会让东西没处看、没处管的，返回拦住的原因；能关返回 null。
 * `draft` 是设置页当前草稿（判断「另一个协议端是不是也关了」）。
 */
export function featureOffBlock(key: FeatureKey, draft: FeatureToggles, usage: FeatureUsage): string | null {
    switch (key) {
        case 'napcat':
        case 'snowluma': {
            const other = key === 'napcat' ? 'snowluma' : 'napcat';
            if (!draft[other]) return '两个协议端至少留一个';
            const n = usage.botsByBackend[key];
            return n > 0 ? `有 ${n} 个 Bot 在用它，先删掉这些 Bot 再关` : null;
        }
        case 'apps':
            return usage.activeAppInstances > 0
                ? `有 ${usage.activeAppInstances} 个实例在运行或安装，先到应用端页停掉再关`
                : null;
        case 'ncdWatch':
            // 心跳停了，装着的 watch 会以为桌面端退出了、自己发告警，和桌面端的重复
            return usage.hostsWithNcdWatch > 0
                ? `有 ${usage.hostsWithNcdWatch} 台远端主机装着 ncd-watch，先到组件页卸载再关`
                : null;
        default:
            return null;
    }
}

/** 能关，但关了会顺手带走点东西的，保存前先说一声。 */
export function featureOffWarning(key: FeatureKey, usage: FeatureUsage): string | null {
    if (key === 'terminal' && usage.openTerminals > 0) {
        return `保存后会关掉开着的 ${usage.openTerminals} 个终端`;
    }
    return null;
}

export function appFrameworkOffBlock(frameworkId: string, usage: FeatureUsage): string | null {
    const n = usage.instancesByFramework[frameworkId] ?? 0;
    return n > 0 ? `有 ${n} 个实例，先删掉再关` : null;
}
