// 设置页。两类设置共存：
//   1. 纯客户端偏好（preferencesStore，localStorage）——主题 / 吉祥物 / 窗口不透明度 /
//      关闭行为，切换即时生效，无保存按钮。
//   2. 后端持久化设置（useBackendSettings）——Bot 登录检查间隔 / 性能监控 / GitHub PAT，
//      走"草稿 + StickySaveBar 保存"模式，保存成功走全局 InfoBar。
//
// 视觉骨架沿用 shadcn Settings recipe：Tabs 横向分类，每行 FieldRow 左标题右控件，
// 行间 hairline 分隔。容器宽度由 AppNext 的 main 控制，本页不自行限宽。
//
// 后端设置草稿：进页时从 server 值拷一份本地草稿，改动累积到草稿，点保存才落盘。
// server 值变化（首次加载完成 / 保存回写）时同步草稿，避免拿到空值。

import { useEffect, useState } from 'react';
import { Tabs, TabsContent, TabsList, TabsTrigger, StickySaveBar } from '../../shared/ui';
import { usePreferences } from '../../hooks/preferences/preferencesStore';
import { useBackendSettings } from '../../hooks/preferences/useBackendSettings';
import type { BackendSettings } from '../../core/services/settings.service';
import { useBootstrap } from '../../hooks/bootstrap/useBootstrap';
import { GeneralTab } from './tabs/GeneralTab';
import { NetworkTab } from './tabs/NetworkTab';
import { DataTab } from './tabs/DataTab';
import { AboutTab } from './tabs/AboutTab';

export function SettingsPageNext() {
    const prefs = usePreferences();
    const { bootstrap, openDataDir, isOpeningDir } = useBootstrap();
    const { settings, save, isSaving } = useBackendSettings();
    const [tab, setTab] = useState('general');

    // 后端设置草稿。server 值首次到达 / 保存回写时同步进草稿。
    const [draft, setDraft] = useState<BackendSettings | null>(null);
    useEffect(() => {
        if (settings) setDraft(settings);
    }, [settings]);

    // 草稿与 server 值是否有差异（控制 StickySaveBar 显隐 + 保存按钮可点）。
    const dirty =
        draft !== null &&
        settings !== null &&
        (draft.botLoginCheckIntervalMs !== settings.botLoginCheckIntervalMs ||
            draft.performanceMonitorEnabled !== settings.performanceMonitorEnabled ||
            draft.performanceMonitorIntervalMs !== settings.performanceMonitorIntervalMs ||
            draft.githubPat !== settings.githubPat);

    const patchDraft = (patch: Partial<BackendSettings>) =>
        setDraft((cur) => (cur ? { ...cur, ...patch } : cur));

    const handleSave = () => {
        if (draft) save(draft);
    };
    const handleCancel = () => {
        if (settings) setDraft(settings);
    };

    return (
        <div className="flex h-full min-h-0 flex-col">
            <header className="shrink-0 pb-4 pt-2">
                <h1 className="font-display text-xl font-semibold leading-none text-text">
                    设置
                </h1>
                <p className="mt-1.5 text-[13px] text-text-secondary">
                    客户端偏好与系统环境
                </p>
            </header>

            <div className="min-h-0 flex-1 overflow-y-auto pr-1">
                <Tabs value={tab} onValueChange={setTab} className="w-full">
                    <TabsList className="mb-6">
                        <TabsTrigger value="general">通用</TabsTrigger>
                        <TabsTrigger value="network">网络</TabsTrigger>
                        <TabsTrigger value="data">数据</TabsTrigger>
                        <TabsTrigger value="about">关于</TabsTrigger>
                    </TabsList>

                    <TabsContent value="general" className="space-y-6">
                        <GeneralTab prefs={prefs} draft={draft} patchDraft={patchDraft} />
                    </TabsContent>

                    <TabsContent value="network" className="space-y-6">
                        <NetworkTab draft={draft} patchDraft={patchDraft} />
                    </TabsContent>

                    <TabsContent value="data" className="space-y-6">
                        <DataTab
                            dataRoot={bootstrap?.data_root ?? '—'}
                            onOpenDataDir={openDataDir}
                            isOpeningDir={isOpeningDir}
                        />
                    </TabsContent>

                    <TabsContent value="about" className="space-y-6">
                        <AboutTab localVersions={bootstrap?.local_versions ?? null} />
                    </TabsContent>
                </Tabs>
            </div>

            <StickySaveBar
                dirty={dirty}
                saving={isSaving}
                onSave={handleSave}
                onCancel={handleCancel}
                saveLabel="保存设置"
            />
        </div>
    );
}

export default SettingsPageNext;
