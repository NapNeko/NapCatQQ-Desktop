import type { ComponentType } from 'react';
import type { AppConfigIssue, AppInstance } from '../../../core/ipc/types';
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

export function buildDetailNav(ui: FrameworkUiModule | undefined): FrameworkNavGroup[] {
    const groups = (ui?.nav ?? []).map((g) => ({ ...g, items: [...g.items] }));
    const instance = groups.find((g) => g.id === INSTANCE_GROUP_ID);
    if (instance) instance.items.push(...SHELL_TABS);
    else groups.push({ id: INSTANCE_GROUP_ID, label: '实例', items: [...SHELL_TABS] });
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
