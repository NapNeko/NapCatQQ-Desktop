// 设置页。所有可编辑项（本机偏好 + 后端 app-settings）共用一个草稿，
// 仅通过右上角「保存设置 / 撤销」落盘；数据 / 关于 Tab 无草稿项。

import { useEffect, useState } from 'react';
import { Save, AlertCircle, Check } from 'lucide-react';
import { Tabs, TabsContent, TabsList, TabsTrigger, Button, Spinner } from '../../shared/ui';
import { MotionIcon, infoToneMotion } from '../../shared/ui/motion';
import { usePreferences } from '../../hooks/preferences/preferencesStore';
import { useBackendSettings } from '../../hooks/preferences/useBackendSettings';
import { useBootstrap } from '../../hooks/bootstrap/useBootstrap';
import { AppearanceTab } from './tabs/AppearanceTab';
import { RuntimeTab } from './tabs/RuntimeTab';
import { DataTab } from './tabs/DataTab';
import { useDesktopLogViewer } from '../../hooks/diagnostics/useDesktopLogViewer';
import { DesktopLogTab } from './tabs/DesktopLogTab';
import { DesktopLogToolbar } from './tabs/DesktopLogToolbar';
import { AboutTab } from './tabs/AboutTab';
import {
    draftFromBackendAndPrefs,
    isSettingsDirty,
    type SettingsDraft,
} from './settings-draft';

export function SettingsPageNext() {
    const prefs = usePreferences();
    const { bootstrap, openDataDir, isOpeningDir } = useBootstrap();
    const { settings, save, isSaving } = useBackendSettings();

    const [tab, setTab] = useState('appearance');
    const [draft, setDraft] = useState<SettingsDraft | null>(null);
    const logViewer = useDesktopLogViewer(tab === 'log');

    useEffect(() => {
        if (settings) {
            setDraft(draftFromBackendAndPrefs(settings, prefs));
        }
        // 仅在后端设置从 IPC 到达或保存回写时同步草稿，不把 prefs 列入依赖以免编辑中被覆盖。
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [settings]);

    const dirty =
        draft !== null && settings !== null && isSettingsDirty(draft, settings, prefs);

    const patchDraft = (patch: Partial<SettingsDraft>) =>
        setDraft((cur) => (cur ? { ...cur, ...patch } : cur));

    const handleSave = () => {
        if (draft) save(draft);
    };

    const handleCancel = () => {
        if (settings) setDraft(draftFromBackendAndPrefs(settings, prefs));
    };

    return (
        <div className="flex h-full min-h-0 w-full flex-col">
            <header className="shrink-0 pb-3 pt-2">
                <h1 className="font-display text-xl font-semibold leading-none text-text">
                    设置
                </h1>
                <p className="mt-1.5 text-[13px] text-text-secondary">
                    外观与运行项修改后请保存；数据页中目录与导入导出可即时操作
                </p>
            </header>

            <Tabs
                value={tab}
                onValueChange={setTab}
                className="flex min-h-0 flex-1 flex-col"
            >
                <div className="sticky top-0 z-[5] shrink-0 border-b border-border-subtle bg-canvas/95 backdrop-blur-sm">
                    <div className="flex items-center gap-2">
                        <TabsList className="shrink-0 border-b-0">
                            <TabsTrigger value="appearance">外观</TabsTrigger>
                            <TabsTrigger value="runtime">运行</TabsTrigger>
                            <TabsTrigger value="log">日志</TabsTrigger>
                            <TabsTrigger value="data">数据</TabsTrigger>
                            <TabsTrigger value="about">关于</TabsTrigger>
                        </TabsList>
                        {tab === 'log' ? null : (
                            <SaveActions
                                dirty={dirty}
                                saving={isSaving}
                                onSave={handleSave}
                                onCancel={handleCancel}
                            />
                        )}
                    </div>
                    {tab === 'log' ? <DesktopLogToolbar {...logViewer} /> : null}
                </div>

                <div
                    className={
                        tab === 'log'
                            ? 'flex min-h-0 flex-1 flex-col px-0.5 pr-2 pb-4'
                            : 'scrollbar-hide min-h-0 flex-1 overflow-y-auto px-0.5 pr-2'
                    }
                >
                    <TabsContent value="appearance" className="pb-10 pt-7 focus-visible:outline-none">
                        <AppearanceTab draft={draft} patchDraft={patchDraft} />
                    </TabsContent>

                    <TabsContent value="runtime" className="pb-10 pt-7 focus-visible:outline-none">
                        <RuntimeTab draft={draft} patchDraft={patchDraft} />
                    </TabsContent>

                    <TabsContent
                        value="log"
                        className="flex min-h-0 flex-1 flex-col pt-0 focus-visible:outline-none"
                    >
                        {tab === 'log' ? (
                            <DesktopLogTab
                                emptyKind={logViewer.emptyKind}
                                displayText={logViewer.displayText}
                                fontSize={logViewer.fontSize}
                                viewportRef={logViewer.viewportRef}
                                error={logViewer.error}
                            />
                        ) : null}
                    </TabsContent>

                    <TabsContent value="data" className="pb-10 pt-7 focus-visible:outline-none">
                        <DataTab
                            dataRoot={bootstrap?.data_root ?? '—'}
                            onOpenDataDir={openDataDir}
                            isOpeningDir={isOpeningDir}
                            draft={draft}
                            patchDraft={patchDraft}
                        />
                    </TabsContent>

                    <TabsContent value="about" className="pb-10 pt-7 focus-visible:outline-none">
                        <AboutTab />
                    </TabsContent>
                </div>
            </Tabs>
        </div>
    );
}

interface SaveActionsProps {
    dirty: boolean;
    saving: boolean;
    onSave: () => void;
    onCancel: () => void;
}

function SaveActions({ dirty, saving, onSave, onCancel }: SaveActionsProps) {
    return (
        <div className="ml-auto flex shrink-0 items-center gap-3 pr-1">
            <span className="hidden text-xs sm:inline-flex sm:items-center sm:gap-1.5">
                {dirty ? (
                    <>
                        <MotionIcon
                            icon={AlertCircle}
                            motion={infoToneMotion('warning')}
                            playEnter={false}
                            size={12}
                            strokeWidth={2.4}
                            className="text-warning"
                        />
                        <span className="text-warning">未保存</span>
                    </>
                ) : (
                    <>
                        <MotionIcon
                            icon={Check}
                            motion={infoToneMotion('success')}
                            playEnter={false}
                            size={12}
                            strokeWidth={2.4}
                            className="text-text-tertiary"
                        />
                        <span className="text-text-tertiary">已是最新</span>
                    </>
                )}
            </span>
            <div className="flex items-center gap-1.5">
                <Button
                    variant="ghost"
                    size="sm"
                    onClick={onCancel}
                    disabled={!dirty || saving}
                >
                    撤销
                </Button>
                <Button
                    variant="primary"
                    size="sm"
                    onClick={onSave}
                    disabled={!dirty || saving}
                >
                    {saving ? (
                        <>
                            <Spinner size="xs" />
                            <span>保存中</span>
                        </>
                    ) : (
                        <>
                            <Save size={13} strokeWidth={2.2} />
                            <span>保存设置</span>
                        </>
                    )}
                </Button>
            </div>
        </div>
    );
}

export default SettingsPageNext;