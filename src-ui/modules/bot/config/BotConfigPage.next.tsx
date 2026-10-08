// Bot 配置页壳：身份 / 连接 / 高级 + 粘性保存。连接编辑态在 ConnectionsTab。
// services 直连（botService / snowlumaAppService）按 eslint 白名单留在本文件，注入子 hook。

import { useEffect, useMemo, useRef, useState } from 'react';
import { Tabs, TabsList, TabsTrigger, TabsContent } from '../../../shared/ui';
import { pushInfoBar } from '../../../hooks/ui/globalInfoBarStore';
import { pushErrorBar } from '../../../hooks/ui/pushErrorBar';
import { useBotConfig } from '../../../hooks/bot/useBotConfig';
import { useBotSnapshots } from '../../../hooks/bot/useBotSnapshots';
import { isBotRunning, isBotStarting } from '../../../core/domain/bot/status';
import {
    createDefaultBotConfig,
    defaultStatusCommandConfig,
} from '../../../core/domain/bot/config-defaults';
import { useBotDockerStartGate } from '../../../hooks/bot/useBotDockerStartGate';
import { useBotRuntimeStartGate } from '../../../hooks/bot/useBotRuntimeStartGate';
import { botService } from '../../../core/services/bot.service';
import { snowlumaAppService } from '../../../core/services/snowlumaApp.service';
import type { StatusCommandConfig } from '../../../core/ipc/generated/domain/StatusCommandConfig';
import { describeSaveResult } from '../../../core/domain/bot/save-result';
import type { BotConfig } from '../../../core/ipc/generated/domain/BotConfig';
import { IdentityTab } from './next/IdentityTab';
import { ConnectionsTab } from './next/ConnectionsTab';
import { AdvancedTab } from './next/AdvancedTab';
import { BotConfigHeader } from './next/BotConfigHeader';
import { ConnectionCountBadge, SaveActions } from './next/BotConfigSaveActions';
import { ConfigLoadingView, ConfigLoadErrorView } from './next/BotConfigLoadError';
import { DeleteBotConfirmDialog } from './next/DeleteBotConfirmDialog';
import { useBotConfigSnowlumaApp } from './next/useBotConfigSnowlumaApp';
import { useBotConfigSaveFlow } from './next/useBotConfigSaveFlow';
import { useBotRemoteNetworkImport } from './next/useBotRemoteNetworkImport';
import { applyServerAppLinks, normalizeLoadedConfig } from './next/botConfigSync';
import { ConfigDriftDialog } from '../dialogs/ConfigDriftDialog';
import { RemoteNetworkPullDialog } from '../dialogs/RemoteNetworkPullDialog';
import { BOT_TOUR_DEMO } from '../../../hooks/desktop/botTourBridge';

interface BotConfigPageNextProps {
    botId: string | null;
    onBack: () => void;
    /** 保存成功后留在配置页；新建时由父级把 botId 设为刚写入的 QQ 号。 */
    onSavedStay?: (savedBotId: string) => void;
    /** 入门引导演示新建：预填 + 拦截保存，不落盘 */
    tourDemoMode?: boolean;
    /** 引导强制切到的 Tab */
    tourForceTab?: TabValue | null;
}

type TabValue = 'identity' | 'connections' | 'advanced';

export function BotConfigPageNext({
    botId,
    onBack,
    onSavedStay,
    tourDemoMode = false,
    tourForceTab = null,
}: BotConfigPageNextProps) {
    const isEditMode = botId !== null;
    const formHydratedForBotRef = useRef<string | null>(null);

    const [activeTab, setActiveTab] = useState<TabValue>('identity');
    const [formData, setFormData] = useState<BotConfig>(createDefaultBotConfig());
    const dockerGateMap = useMemo(() => ({ __form__: formData }), [formData]);
    const { saveBlock: dockerSaveBlock } = useBotDockerStartGate(dockerGateMap);

    const runtimeGateMap = useMemo(() => ({ __form__: formData }), [formData]);
    const { saveBlock: runtimeSaveBlock } = useBotRuntimeStartGate(runtimeGateMap);
    const [pristine, setPristine] = useState<BotConfig>(createDefaultBotConfig());
    const [deleteDialogOpen, setDeleteDialogOpen] = useState(false);

    const {
        snowlumaApp,
        setSnowlumaApp,
        snowlumaAppPristine,
        snowlumaAppLoading,
        snowlumaAppLoadError,
        commitIfDirty: commitSnowlumaIfDirty,
    } = useBotConfigSnowlumaApp({
        load: () => snowlumaAppService.get(),
        persist: (config) => snowlumaAppService.set(config),
    });

    // 把当前 bot 的 actor 状态拉过来，IdentityTab 需要根据 Running / Starting
    // 锁住 backend_type Select。复用 useBotSnapshots 的 react-query cache，
    // 跟 BotListPage 共享同一份 query；通常已经在 cache 里，没有额外网络开销。
    const { data: snapshots = [] } = useBotSnapshots();
    const currentSnapshot = useMemo(
        () => (botId ? (snapshots.find((s) => s.bot_id === botId) ?? null) : null),
        [snapshots, botId],
    );
    const isRunning = currentSnapshot
        ? isBotRunning(currentSnapshot.state) ||
          isBotStarting(currentSnapshot.state) ||
          currentSnapshot.state === 'stopping'
        : false;

    const {
        config: loadedConfig,
        isLoading,
        error,
        save,
        saveWithDecisions,
        isSaving,
        remove,
        isDeleting,
    } = useBotConfig(botId, {
        onSaved: (savedBotId, reason) => {
            const desc = describeSaveResult(reason, savedBotId);
            pushInfoBar({
                tone: desc.tone,
                title: desc.title,
                content: desc.content,
                autoDismissMs: 3000,
            });
            onSavedStay?.(savedBotId);
            setPristine(formData);
            formHydratedForBotRef.current = savedBotId;
        },
        onDeleted: () => {
            setDeleteDialogOpen(false);
            pushInfoBar({
                tone: 'success',
                title: '实例已删除',
                content: `Bot ${botId} 已彻底删除`,
                autoDismissMs: 3000,
            });
            onBack();
        },
        onError: (msg) => {
            setDeleteDialogOpen(false);
            pushErrorBar({
                key: 'bot-config-error',
                title: '操作失败',
                raw: msg,
            });
        },
    });

    const dirty = useMemo(() => {
        const botDirty = JSON.stringify(formData) !== JSON.stringify(pristine);
        const snowlumaDirty = JSON.stringify(snowlumaApp) !== JSON.stringify(snowlumaAppPristine);
        return botDirty || snowlumaDirty;
    }, [formData, pristine, snowlumaApp, snowlumaAppPristine]);
    const dirtyRef = useRef(dirty);
    dirtyRef.current = dirty;

    useEffect(() => {
        if (!error) return;
        pushErrorBar({
            key: 'bot-config-load',
            title: '读取配置失败',
            raw: error.message,
        });
    }, [error]);

    // 引导强制 Tab（演示新建流程）
    useEffect(() => {
        if (tourForceTab) setActiveTab(tourForceTab);
    }, [tourForceTab]);

    // 编辑态：每个 botId 只从服务端灌一次表单，避免 invalidate 后把用户未保存的改动盖掉。
    // 未 dirty 时允许 loadedConfig 更新后再灌（导入后缓存刷新）。
    useEffect(() => {
        if (!isEditMode) {
            formHydratedForBotRef.current = null;
            const fresh = createDefaultBotConfig();
            if (tourDemoMode) {
                fresh.bot = {
                    ...fresh.bot,
                    QQID: BOT_TOUR_DEMO.qqId,
                    name: BOT_TOUR_DEMO.name,
                    backend_type: 'napcat',
                    runtime_target: 'local',
                };
            }
            setFormData(fresh);
            // 演示：pristine 用空默认，让 dirty=true，保存按钮可点（仍拦截落盘）
            setPristine(tourDemoMode ? createDefaultBotConfig() : fresh);
            return;
        }
        if (!loadedConfig || botId == null) return;
        const normalized = normalizeLoadedConfig(loadedConfig);
        if (formHydratedForBotRef.current === botId) {
            if (!dirtyRef.current) {
                setFormData(normalized);
                setPristine(normalized);
            } else {
                // 对接 / 解绑由后端落盘；脏表单只把 ncd-app:* 跟服务端对齐，
                // 避免随后点保存把已解除的连接写回去，或丢掉刚对接上的那条。
                setFormData((prev) => applyServerAppLinks(prev, normalized));
                setPristine((prev) => applyServerAppLinks(prev, normalized));
            }
            return;
        }
        setFormData(normalized);
        setPristine(normalized);
        formHydratedForBotRef.current = botId;
    }, [loadedConfig, isEditMode, botId, tourDemoMode]);

    const updateBot = (patch: Partial<BotConfig['bot']>) => {
        setFormData((prev) => ({ ...prev, bot: { ...prev.bot, ...patch } }));
    };

    const updateConnect = (patch: Partial<BotConfig['connect']>) => {
        setFormData((prev) => ({ ...prev, connect: { ...prev.connect, ...patch } }));
    };

    const updateAdvanced = (patch: Partial<BotConfig['advanced']>) => {
        setFormData((prev) => ({ ...prev, advanced: { ...prev.advanced, ...patch } }));
    };

    const updateStatusCommand = (patch: Partial<StatusCommandConfig>) => {
        setFormData((prev) => ({
            ...prev,
            statusCommand: {
                ...(prev.statusCommand ?? defaultStatusCommandConfig()),
                ...patch,
            },
        }));
    };

    const {
        canPullRemote,
        handlePullRemote,
        pullingRemote,
        remotePreview,
        confirmPullRemote,
        setRemotePreview,
    } = useBotRemoteNetworkImport({
        botId,
        isEditMode,
        tourDemoMode,
        loadedConfig,
        formData,
        onApply: setFormData,
    });

    const { handleSave, pendingSaveDrift, handleSaveDriftConfirm, handleSaveDriftCancel } =
        useBotConfigSaveFlow({
            isEditMode,
            botId,
            tourDemoMode,
            formData,
            dockerSaveBlock,
            runtimeSaveBlock,
            commitSnowlumaIfDirty,
            detectDrift: (id) => botService.detectConfigDrift(id),
            save,
            saveWithDecisions,
        });

    const handleCancel = () => {
        setFormData(pristine);
        setSnowlumaApp(snowlumaAppPristine);
    };

    // ───── 加载中 / 出错 ─────
    if (isEditMode && isLoading) return <ConfigLoadingView />;
    if (isEditMode && error) return <ConfigLoadErrorView onBack={onBack} />;

    return (
        <div className="flex h-full w-full flex-col">
            <BotConfigHeader
                isEditMode={isEditMode}
                tourDemoMode={tourDemoMode}
                botId={botId}
                backendType={formData.bot.backend_type}
                runtimeTarget={formData.bot.runtime_target}
                onBack={onBack}
                onRequestDelete={() => setDeleteDialogOpen(true)}
            />

            {/* ────── Tabs + 主体 ────── */}
            <div className="flex min-h-0 flex-1 flex-col overflow-y-auto px-2">
                <div className="flex flex-1 flex-col">
                    <Tabs
                        value={activeTab}
                        onValueChange={(v) => {
                            if (tourDemoMode && tourForceTab) return;
                            setActiveTab(v as TabValue);
                        }}
                        className="flex flex-1 flex-col"
                    >
                        <div className="sticky top-0 z-[5] flex items-center justify-between gap-3 border-b border-border-subtle bg-canvas/95 backdrop-blur-sm">
                            <TabsList className="border-b-0">
                                <TabsTrigger value="identity">身份</TabsTrigger>
                                <TabsTrigger value="connections" data-tour-id="bot-connections-tab">
                                    连接
                                    <ConnectionCountBadge config={formData} />
                                </TabsTrigger>
                                <TabsTrigger value="advanced">高级</TabsTrigger>
                            </TabsList>
                            <SaveActions
                                dirty={dirty}
                                saving={isSaving}
                                onSave={handleSave}
                                onCancel={handleCancel}
                                tourDemoMode={tourDemoMode}
                            />
                        </div>

                        <TabsContent value="identity" className="pb-8 pt-2">
                            <IdentityTab
                                data={formData.bot}
                                onChange={updateBot}
                                isEditMode={isEditMode}
                                isRunning={isRunning}
                            />
                        </TabsContent>
                        <TabsContent value="connections" className="pb-8 pt-2">
                            {/* 锚点必须在 TabsContent 子树内：TabsContent 非激活不挂载，且 asChild 不转发 data-tour-id */}
                            <div data-tour-id="bot-connections-body" className="min-h-[12rem]">
                                <ConnectionsTab
                                    data={formData.connect}
                                    onChange={updateConnect}
                                    backendType={formData.bot.backend_type}
                                    botId={tourDemoMode ? null : botId}
                                    onPullRemote={canPullRemote ? handlePullRemote : undefined}
                                    pullingRemote={pullingRemote}
                                />
                            </div>
                        </TabsContent>
                        <TabsContent value="advanced" className="pb-8 pt-2">
                            <AdvancedTab
                                data={formData.advanced}
                                onChange={updateAdvanced}
                                backendType={formData.bot.backend_type}
                                statusCommand={formData.statusCommand ?? null}
                                onStatusCommandChange={updateStatusCommand}
                                webuiPasswordTakeover={formData.bot.webuiPasswordTakeover}
                                onWebuiPasswordTakeoverChange={(v) =>
                                    updateBot({ webuiPasswordTakeover: v })
                                }
                                snowlumaAppConfig={snowlumaApp}
                                onSnowlumaAppConfigChange={setSnowlumaApp}
                                snowlumaAppLoadError={snowlumaAppLoadError}
                                snowlumaAppLoading={snowlumaAppLoading}
                            />
                        </TabsContent>
                    </Tabs>
                </div>
            </div>

            {/* 底部 dock：connections tab 的 portal 挂载点；其它 tab 这里为空 */}
            <div id="connections-add-dock" />

            {/* ────── 删除二次确认 ────── */}
            <DeleteBotConfirmDialog
                open={deleteDialogOpen}
                onOpenChange={setDeleteDialogOpen}
                botId={botId}
                onDelete={remove}
                isDeleting={isDeleting}
            />

            {/* 保存时的 drift 确认 */}
            {pendingSaveDrift && (
                <ConfigDriftDialog
                    open={!!pendingSaveDrift}
                    drift={pendingSaveDrift}
                    intent="save"
                    onConfirm={handleSaveDriftConfirm}
                    onCancel={handleSaveDriftCancel}
                />
            )}

            <RemoteNetworkPullDialog
                preview={remotePreview}
                onConfirm={confirmPullRemote}
                onCancel={() => setRemotePreview(null)}
            />
        </div>
    );
}
