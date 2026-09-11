import type { ComponentType } from 'react';
import type { AppConfigIssue, AppInstance } from '../../../core/ipc/types';
import { astrbotFrameworkUi } from './astrbot/astrbotFrameworkUi';
import { karinFrameworkUi } from './karin/karinFrameworkUi';
import { nonebot2FrameworkUi } from './nonebot2/nonebot2FrameworkUi';

export type FrameworkTabDef = { value: string; label: string };

export type FrameworkSaveHandle = {
    dirty: boolean;
    saving: boolean;
    issueCount: number;
    save: (overwrite?: boolean) => Promise<{ kind: string; issues?: AppConfigIssue[] }>;
    reset: () => void;
    conflict: boolean;
    dismissConflict: () => void;
    reloadDiscard: () => void | Promise<void>;
};

export type FrameworkDetailProps = {
    instance: AppInstance;
    onSaveHandle: (handle: FrameworkSaveHandle | null) => void;
    /** 切到本框架的某个 Tab；空态 / 就绪清单里「去 X 页」用它，不再让用户自己找 */
    onGoTab: (tab: string) => void;
};

export type FrameworkUiModule = {
    extraTabs: readonly FrameworkTabDef[];
    defaultTab: string;
    typedTabs: ReadonlySet<string>;
    fillPaneTabs: ReadonlySet<string>;
    tabForIssue: (path: string) => string;
    Detail: ComponentType<FrameworkDetailProps>;
};

const MODULES: Record<string, FrameworkUiModule> = {
    karin: karinFrameworkUi,
    nonebot2: nonebot2FrameworkUi,
    astrbot: astrbotFrameworkUi,
};

export function resolveFrameworkUi(frameworkId: string): FrameworkUiModule | undefined {
    return MODULES[frameworkId];
}
