// 设置页。所有可编辑项（本机偏好 + 后端 app-settings）共用一个草稿，
// 仅通过右上角「保存设置 / 撤销」落盘；数据 / 关于 Tab 无草稿项。

import { useEffect, useState } from 'react';
import { Save, AlertCircle, Check } from 'lucide-react';
import { Tabs, TabsContent, TabsList, TabsTrigger, Button, Spinner } from '../../shared/ui';
import { usePreferences } from '../../hooks/preferences/preferencesStore';
import { useBackendSettings } from '../../hooks/preferences/useBackendSettings';
import { useBootstrap } from '../../hooks/bootstrap/useBootstrap';
import { GeneralTab } from './tabs/GeneralTab';
import { NetworkTab } from './tabs/NetworkTab';
import { DataTab } from './tabs/DataTab';
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

    const [tab, setTab] = useState('general');
    const [draft, setDraft] = useState<SettingsDraft | null>(null);

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
                    修改后请点击右上角保存；数据与关于页为即时操作
                </p>
            </header>

            <Tabs
                value={tab}
                onValueChange={setTab}
                className="flex min-h-0 flex-1 flex-col"
            >
                <div className="sticky top-0 z-[5] flex shrink-0 items-center justify-between gap-3 border-b border-border-subtle bg-canvas/95 backdrop-blur-sm">
                    <TabsList className="border-b-0">
                        <TabsTrigger value="general">通用</TabsTrigger>
                        <TabsTrigger value="network">网络</TabsTrigger>
                        <TabsTrigger value="data">数据</TabsTrigger>
                        <TabsTrigger value="about">关于</TabsTrigger>
                    </TabsList>
                    <SaveActions
                        dirty={dirty}
                        saving={isSaving}
                        onSave={handleSave}
                        onCancel={handleCancel}
                    />
                </div>

                <div className="scrollbar-hide min-h-0 flex-1 overflow-y-auto pr-1">
                    <TabsContent value="general" className="space-y-6 pb-8 pt-6">
                        <GeneralTab draft={draft} patchDraft={patchDraft} />
                    </TabsContent>

                    <TabsContent value="network" className="space-y-6 pb-8 pt-6">
                        <NetworkTab draft={draft} patchDraft={patchDraft} />
                    </TabsContent>

                    <TabsContent value="data" className="space-y-6 pb-8 pt-6">
                        <DataTab
                            dataRoot={bootstrap?.data_root ?? '—'}
                            onOpenDataDir={openDataDir}
                            isOpeningDir={isOpeningDir}
                        />
                    </TabsContent>

                    <TabsContent value="about" className="space-y-6 pb-8 pt-6">
                        <AboutTab localVersions={bootstrap?.local_versions ?? null} />
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
        <div className="flex shrink-0 items-center gap-3 pr-1">
            <span className="hidden text-xs sm:inline-flex sm:items-center sm:gap-1.5">
                {dirty ? (
                    <>
                        <AlertCircle size={12} strokeWidth={2.4} className="text-warning" />
                        <span className="text-warning">未保存</span>
                    </>
                ) : (
                    <>
                        <Check size={12} strokeWidth={2.4} className="text-text-tertiary" />
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