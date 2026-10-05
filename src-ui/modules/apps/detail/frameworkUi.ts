import type { ComponentType } from 'react';
import type { AppConfigIssue, AppInstance } from '../../../core/ipc/types';
import type { AppRoute } from '../../../shared/components/next/Sidebar';
import { astrbotFrameworkUi } from './astrbot/astrbotFrameworkUi';
import { karinFrameworkUi } from './karin/karinFrameworkUi';
import { koishiFrameworkUi } from './koishi/koishiFrameworkUi';
import { maibotFrameworkUi } from './maibot/maibotFrameworkUi';
import { neobotFrameworkUi } from './neobot/neobotFrameworkUi';
import { nonebot2FrameworkUi } from './nonebot2/nonebot2FrameworkUi';
import { yunzaiFrameworkUi } from './yunzai/yunzaiFrameworkUi';

export type FrameworkTabDef = { value: string; label: string };

/** 侧栏一组。label 为空就不画组标题（概览这种单独一项）。 */
export type FrameworkNavGroup = { id: string; label?: string; items: readonly FrameworkTabDef[] };

/** 侧栏小圆点：next = 下一步在这页，warn = 这页有不挡运行的配置冲突，error = 这页有填错、挡着保存的字段 */
export type NavBadgeTone = 'next' | 'warn' | 'error';
export type NavBadges = Readonly<Partial<Record<string, NavBadgeTone>>>;

export type FrameworkSaveHandle = {
    dirty: boolean;
    saving: boolean;
    issueCount: number;
    /** 前端校验不过的字段路径；外壳用 tabForIssue 换成页，亮红点、给保存条「去看看」 */
    issuePaths: readonly string[];
    save: (overwrite?: boolean) => Promise<{ kind: string; issues?: AppConfigIssue[] }>;
    reset: () => void;
    conflict: boolean;
    dismissConflict: () => void;
    reloadDiscard: () => void | Promise<void>;
};

export type FrameworkDetailProps = {
    instance: AppInstance;
    onSaveHandle: (handle: FrameworkSaveHandle | null) => void;
    /** 切到本框架的某一页；空态 / 清单里「去 X 页」用它，不再让用户自己找 */
    onGoTab: (tab: string) => void;
    /** 打开外壳的对接对话框（概览清单「连上 QQ」用） */
    onOpenLink: () => void;
    /** 打开框架自带的 Web 控制台（外壳负责开隧道 / 复制凭据） */
    onOpenWebUi: () => void;
    /** 跳到别的页（例如「去机器人页新建」）；外壳没给就不显示这类按钮 */
    onNavigate?: (route: AppRoute) => void;
    /** 上报侧栏小圆点；不关心的框架不调 */
    onNavBadges: (badges: NavBadges) => void;
};

export type FrameworkUiModule = {
    nav: readonly FrameworkNavGroup[];
    defaultTab: string;
    typedTabs: ReadonlySet<string>;
    fillPaneTabs: ReadonlySet<string>;
    tabForIssue: (path: string) => string;
    Detail: ComponentType<FrameworkDetailProps>;
};

/** 原始文件、日志由外壳自己渲染，所有框架都挂在这一组末尾 */
const INSTANCE_GROUP_ID = 'instance';
const SHELL_TABS: readonly FrameworkTabDef[] = [
    { value: 'raw', label: '原始文件' },
    { value: 'log', label: '日志' },
];

/** 「版本」页也由外壳渲染（支持按版本安装的框架共用），排在实例组最前 */
const VERSION_TAB: FrameworkTabDef = { value: 'version', label: '版本' };

/**
 * 框架给的分组 + 外壳追加的页。
 *
 * withVersionTab 由详情页按「这个框架支不支持按版本安装」传进来：不支持就没有可选项，
 * 摆一个空页只会让人以为坏了。
 */
export function buildDetailNav(
    ui: FrameworkUiModule | undefined,
    options?: { withVersionTab?: boolean },
): FrameworkNavGroup[] {
    const shellTabs = options?.withVersionTab ? [VERSION_TAB, ...SHELL_TABS] : [...SHELL_TABS];
    const groups = (ui?.nav ?? []).map((g) => ({ ...g, items: [...g.items] }));
    const instance = groups.find((g) => g.id === INSTANCE_GROUP_ID);
    if (instance) instance.items.push(...shellTabs);
    else groups.push({ id: INSTANCE_GROUP_ID, label: '实例', items: shellTabs });
    return groups;
}

const MODULES: Record<string, FrameworkUiModule> = {
    karin: karinFrameworkUi,
    nonebot2: nonebot2FrameworkUi,
    astrbot: astrbotFrameworkUi,
    maibot: maibotFrameworkUi,
    koishi: koishiFrameworkUi,
    yunzai: yunzaiFrameworkUi,
    neobot: neobotFrameworkUi,
};

export function resolveFrameworkUi(frameworkId: string): FrameworkUiModule | undefined {
    return MODULES[frameworkId];
}
