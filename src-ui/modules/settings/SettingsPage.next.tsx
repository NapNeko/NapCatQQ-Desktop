// 设置页。两类设置共存：
//   1. 纯客户端偏好（preferencesStore，localStorage）——主题 / 吉祥物 / 窗口不透明度 /
//      关闭行为，切换即时生效，无保存按钮。
//   2. 后端持久化设置（useBackendSettings）——Bot 登录检查间隔 / 性能监控 / GitHub PAT，
//      走"草稿 + 顶部保存按钮"模式，保存成功走全局 InfoBar。
//
// 视觉骨架沿用 shadcn Settings recipe：Tabs 横向分类，每行 FieldRow 左标题右控件，
// 行间 hairline 分隔。容器宽度由 AppNext 的 main 控制，本页不自行限宽。
//
// 后端设置草稿：进页时从 server 值拷一份本地草稿，改动累积到草稿，点保存才落盘。
// server 值变化（首次加载完成 / 保存回写）时同步草稿，避免拿到空值。

import { useEffect, useState } from 'react';
import { Save, AlertCircle, Check } from 'lucide-react';
import { Tabs, TabsContent, TabsList, TabsTrigger, Button, Spinner } from '../../shared/ui';
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

    // 草稿与 server 值是否有差异（控制顶部保存按钮可点与状态文案）。
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
        <div className="flex h-full min-h-0 w-full flex-col">
            <header className="shrink-0 pb-3 pt-2">
                <h1 className="font-display text-xl font-semibold leading-none text-text">
                    设置
                </h1>
                <p className="mt-1.5 text-[13px] text-text-secondary">
                    客户端偏好与系统环境
                </p>
            </header>

            <Tabs
                value={tab}
                onValueChange={setTab}
                className="flex min-h-0 flex-1 flex-col"
            >
                {/* tab 栏 + 保存/状态同一行，sticky 不跟内容滚动 */}
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

                {/* 内容区独立滚动，tab 栏留在外面 */}
                <div className="scrollbar-hide min-h-0 flex-1 overflow-y-auto pr-1">
                    <TabsContent value="general" className="space-y-6 pb-8 pt-6">
                        <GeneralTab prefs={prefs} draft={draft} patchDraft={patchDraft} />
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

/// 顶部 tab 栏右侧的保存/状态区。始终显示：dirty 时提示未保存 + 按钮可点，
/// 否则显示已是最新 + 按钮 disabled。数据/关于页没有可保存的后端项，dirty
/// 恒为 false，按钮自然 disabled。
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
