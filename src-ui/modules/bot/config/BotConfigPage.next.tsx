// 配置页顶层壳。负责：
//   - header（返回 + 标题副标题 + 删除按钮）
//   - Tabs（身份 / 连接 / 高级）切换
//   - 整页表单 state + dirty 检测
//   - 粘性保存条
//   - 保存 / 删除 mutation 接全局 InfoBar
//   - 加载 / 失败态
//
// 不做的：
//   - 不持有连接列表的 inline 编辑态（在 ConnectionsTab 自己内部）
//   - 不持有 PID picker（HotStart 模式下 backend 自动按 qq_id 匹配 PID）
//
// 状态机：
//   - editMode: botId 非 null
//   - 加载中（仅编辑模式）→ 拉到 → 同步到 formData
//   - 用户改字段 → setFormData → dirty=true
//   - 保存成功 → push 全局 success InfoBar → 留在配置页（同步 pristine / 新建则切到编辑态）
//   - 删除成功 → 父级 onBack（删除走 dialog 二次确认）

import { useCallback, useEffect, useMemo, useState } from 'react';
import { ArrowLeft, Trash2, Save, AlertCircle, Check } from 'lucide-react';
import {
    Button,
    Card,
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
    DialogDescription,
    DialogFooter,
    Tabs,
    TabsList,
    TabsTrigger,
    TabsContent,
    Spinner,
} from '../../../shared/ui';
import { pushInfoBar } from '../../../hooks/ui/globalInfoBarStore';
import { useBotConfig } from '../../../hooks/bot/useBotConfig';
import { useBotSnapshots } from '../../../hooks/bot/useBotSnapshots';
import {
    isBotRunning,
    isBotStarting,
} from '../../../core/domain/bot/status';
import {
    createDefaultBotConfig,
    validateBotConfig,
    defaultStatusCommandConfig,
} from '../../../core/domain/bot/config-defaults';
import type { StatusCommandConfig } from '../../../core/ipc/generated/domain/StatusCommandConfig';
import { describeSaveResult } from '../../../core/domain/bot/save-result';
import { botService } from '../../../core/services/bot.service';
import { snowlumaAppService } from '../../../core/services/snowlumaApp.service';
import type { BotConfig } from '../../../core/ipc/generated/domain/BotConfig';
import type { SnowLumaAppConfig } from '../../../core/ipc/generated/domain/SnowLumaAppConfig';
import type { ConfigDrift } from '../../../core/ipc/generated/ConfigDrift';
import type { DriftDecision } from '../../../core/ipc/generated/DriftDecision';
import {
    ActionMotionIcon,
    infoToneMotion,
} from '../../../shared/ui/motion';
import { IdentityTab } from './next/IdentityTab';
import { ConnectionsTab } from './next/ConnectionsTab';
import { AdvancedTab } from './next/AdvancedTab';
import { ConfigDriftDialog } from '../dialogs/ConfigDriftDialog';

interface BotConfigPageNextProps {
    botId: string | null;
    onBack: () => void;
    /** 保存成功后留在配置页；新建时由父级把 botId 设为刚写入的 QQ 号。 */
    onSavedStay?: (savedBotId: string) => void;
}

type TabValue = 'identity' | 'connections' | 'advanced';

const defaultSnowlumaAppConfig = (): SnowLumaAppConfig => ({
    snowlumaWebuiPasswordOverride: '',
    snowlumaWebuiPort: 5099,
});

export function BotConfigPageNext({ botId, onBack, onSavedStay }: BotConfigPageNextProps) {
    const isEditMode = botId !== null;

    const [activeTab, setActiveTab] = useState<TabValue>('identity');
    const [formData, setFormData] = useState<BotConfig>(createDefaultBotConfig());
    const [pristine, setPristine] = useState<BotConfig>(createDefaultBotConfig());
    const [deleteDialogOpen, setDeleteDialogOpen] = useState(false);

    const [snowlumaApp, setSnowlumaApp] = useState<SnowLumaAppConfig>(defaultSnowlumaAppConfig);
    const [snowlumaAppPristine, setSnowlumaAppPristine] =
        useState<SnowLumaAppConfig>(defaultSnowlumaAppConfig);
    const [snowlumaAppLoading, setSnowlumaAppLoading] = useState(true);
    const [snowlumaAppLoadError, setSnowlumaAppLoadError] = useState<string | null>(null);

    useEffect(() => {
        let cancelled = false;
        (async () => {
            setSnowlumaAppLoading(true);
            setSnowlumaAppLoadError(null);
            try {
                const loaded = await snowlumaAppService.get();
                if (cancelled) return;
                setSnowlumaApp(loaded);
                setSnowlumaAppPristine(loaded);
            } catch (e) {
                if (!cancelled) setSnowlumaAppLoadError(String(e));
            } finally {
                if (!cancelled) setSnowlumaAppLoading(false);
            }
        })();
        return () => {
            cancelled = true;
        };
    }, []);

    // 把当前 bot 的 actor 状态拉过来，IdentityTab 需要根据 Running / Starting
    // 锁住 backend_type Select。复用 useBotSnapshots 的 react-query cache，
    // 跟 BotListPage 共享同一份 query；通常已经在 cache 里，没有额外网络开销。
    const { data: snapshots = [] } = useBotSnapshots();
    const currentSnapshot = useMemo(
        () => (botId ? snapshots.find((s) => s.bot_id === botId) ?? null : null),
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
            pushInfoBar({
                tone: 'danger',
                title: '操作失败',
                content: msg,
                key: 'bot-config-error',
            });
        },
    });

    // 服务端拉到配置后同步进 form + pristine 基线
    useEffect(() => {
        if (loadedConfig) {
            const normalized = normalizeLoadedConfig(loadedConfig);
            setFormData(normalized);
            setPristine(normalized);
        } else if (!isEditMode) {
            const fresh = createDefaultBotConfig();
            setFormData(fresh);
            setPristine(fresh);
        }
    }, [loadedConfig, isEditMode]);

    const dirty = useMemo(() => {
        const botDirty = JSON.stringify(formData) !== JSON.stringify(pristine);
        const snowlumaDirty =
            JSON.stringify(snowlumaApp) !== JSON.stringify(snowlumaAppPristine);
        return botDirty || snowlumaDirty;
    }, [formData, pristine, snowlumaApp, snowlumaAppPristine]);

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

    const normalizeLoadedConfig = (c: BotConfig): BotConfig => {
        if (c.bot.backend_type === 'snowluma' && !c.statusCommand) {
            return { ...c, statusCommand: defaultStatusCommandConfig() };
        }
        return c;
    };

    const commitSnowlumaIfDirty = async (): Promise<void> => {
        if (JSON.stringify(snowlumaApp) === JSON.stringify(snowlumaAppPristine)) return;
        await snowlumaAppService.set(snowlumaApp);
        setSnowlumaAppPristine(snowlumaApp);
    };

    const handleSave = async () => {
        // 实例名为空时用 placeholder 兜底(后端不允许空 name)
        const finalData: BotConfig = {
            ...formData,
            bot: {
                ...formData.bot,
                name: formData.bot.name.trim() || `Bot-${String(formData.bot.QQID).slice(-4)}`,
            },
            statusCommand:
                formData.bot.backend_type === 'snowluma'
                    ? formData.statusCommand ?? defaultStatusCommandConfig()
                    : formData.statusCommand,
        };

        const validation = validateBotConfig(finalData);
        if (!validation.ok) {
            pushInfoBar({
                tone: 'danger',
                title: '配置不通过',
                content: validation.reason,
                key: 'bot-config-error',
            });
            return;
        }

        try {
            await commitSnowlumaIfDirty();
        } catch (e) {
            pushInfoBar({
                tone: 'danger',
                title: '保存失败',
                content: `全局 WebUI 配置写入失败：${String(e)}`,
                key: 'bot-config-error',
            });
            return;
        }

        // 编辑模式下检测 drift
        if (isEditMode && botId) {
            try {
                const drift = await botService.detectConfigDrift(botId);
                if (drift && (drift.added.length > 0 || drift.modified.length > 0)) {
                    // 有 drift,弹 dialog
                    setPendingSaveData(finalData);
                    setPendingSaveDrift(drift);
                    return;
                }
            } catch {
                // 检测失败不阻塞保存,直接走
            }
        }
        save(finalData);
    };

    // Drift dialog state for save
    const [pendingSaveDrift, setPendingSaveDrift] = useState<ConfigDrift | null>(null);
    const [pendingSaveData, setPendingSaveData] = useState<BotConfig | null>(null);

    const handleSaveDriftConfirm = useCallback(
        async (decisions: DriftDecision[]) => {
            if (!pendingSaveData) return;
            try {
                await commitSnowlumaIfDirty();
            } catch (e) {
                pushInfoBar({
                    tone: 'danger',
                    title: '保存失败',
                    content: `全局 WebUI 配置写入失败：${String(e)}`,
                    key: 'bot-config-error',
                });
                return;
            }
            setPendingSaveDrift(null);
            saveWithDecisions(pendingSaveData, decisions);
            setPendingSaveData(null);
        },
        [pendingSaveData, saveWithDecisions, snowlumaApp, snowlumaAppPristine],
    );

    const handleSaveDriftCancel = useCallback(() => {
        setPendingSaveDrift(null);
        setPendingSaveData(null);
    }, []);

    const handleCancel = () => {
        setFormData(pristine);
        setSnowlumaApp(snowlumaAppPristine);
    };

    // ───── 加载中 / 出错 ─────
    if (isEditMode && isLoading) {
        return (
            <div className="flex h-full items-center justify-center">
                <Card className="flex flex-col items-center gap-3 px-10 py-8" variant="default">
                    <Spinner size="lg" tone="brand" />
                    <p className="text-sm text-text-secondary">正在读取配置文件…</p>
                </Card>
            </div>
        );
    }

    if (isEditMode && error) {
        return (
            <div className="flex h-full items-center justify-center">
                <Card className="flex max-w-md flex-col gap-3 px-6 py-5" variant="outlined">
                    <h3 className="font-display text-md font-semibold text-danger">读取配置失败</h3>
                    <p className="text-sm text-text-secondary">{error.message}</p>
                    <div>
                        <Button variant="secondary" size="sm" onClick={onBack}>
                            返回列表
                        </Button>
                    </div>
                </Card>
            </div>
        );
    }

    return (
        <div className="flex h-full w-full flex-col">
            {/* ────── Header ────── */}
            <header className="flex items-start justify-between gap-3 border-b border-border-subtle py-3">
                <div className="flex items-start gap-3">
                    <Button variant="ghost" size="icon" onClick={onBack} aria-label="返回列表">
                        <ActionMotionIcon icon={ArrowLeft} size={16} />
                    </Button>
                    <div className="flex flex-col gap-0.5">
                        <h1 className="font-display text-md font-semibold text-text">
                            {isEditMode ? '编辑 Bot 配置' : '新建 Bot'}
                        </h1>
                        <p className="text-xs text-text-tertiary">
                            {isEditMode
                                ? `QQ ${botId} · ${formData.bot.backend_type} · ${formData.bot.runtime_target}`
                                : '账号身份 → 连接通道 → 高级选项，至少添加一个连接才能与外部通信'}
                        </p>
                    </div>
                </div>
                {isEditMode && (
                    <Button
                        variant="ghost"
                        size="sm"
                        className="text-danger hover:text-danger"
                        onClick={() => setDeleteDialogOpen(true)}
                    >
                        <ActionMotionIcon icon={Trash2} size={13} strokeWidth={2.2} />
                        <span>删除实例</span>
                    </Button>
                )}
            </header>

            {/* ────── Tabs + 主体 ────── */}
            <div className="flex min-h-0 flex-1 flex-col overflow-y-auto px-2">
                <div className="flex flex-1 flex-col">
                    <Tabs
                        value={activeTab}
                        onValueChange={(v) => setActiveTab(v as TabValue)}
                        className="flex flex-1 flex-col"
                    >
                        <div className="sticky top-0 z-[5] flex items-center justify-between gap-3 border-b border-border-subtle bg-canvas/95 backdrop-blur-sm">
                            <TabsList className="border-b-0">
                                <TabsTrigger value="identity">身份</TabsTrigger>
                                <TabsTrigger value="connections">
                                    连接
                                    <ConnectionCountBadge count={countConnections(formData)} />
                                </TabsTrigger>
                                <TabsTrigger value="advanced">高级</TabsTrigger>
                            </TabsList>
                            <SaveActions
                                dirty={dirty}
                                saving={isSaving}
                                onSave={handleSave}
                                onCancel={handleCancel}
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
                            <ConnectionsTab
                                data={formData.connect}
                                onChange={updateConnect}
                                backendType={formData.bot.backend_type}
                            />
                        </TabsContent>
                        <TabsContent value="advanced" className="pb-8 pt-2">
                            <AdvancedTab
                                data={formData.advanced}
                                onChange={updateAdvanced}
                                backendType={formData.bot.backend_type}
                                statusCommand={formData.statusCommand ?? null}
                                onStatusCommandChange={updateStatusCommand}
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
            <Dialog open={deleteDialogOpen} onOpenChange={setDeleteDialogOpen}>
                <DialogContent className="max-w-sm">
                    <DialogHeader>
                        <DialogTitle>彻底删除该 Bot？</DialogTitle>
                        <DialogDescription>
                            将永久删除 Bot {botId} 的全部配置与数据，运行中的进程会被强制停止。此操作不可撤销。
                        </DialogDescription>
                    </DialogHeader>
                    <DialogFooter>
                        <Button variant="ghost" size="sm" onClick={() => setDeleteDialogOpen(false)}>
                            取消
                        </Button>
                        <Button variant="danger" size="sm" onClick={remove} disabled={isDeleting}>
                            {isDeleting ? '删除中…' : '彻底删除'}
                        </Button>
                    </DialogFooter>
                </DialogContent>
            </Dialog>

            {/* 保存时的 drift 确认 */}
            {pendingSaveDrift && (
                <ConfigDriftDialog
                    open={!!pendingSaveDrift}
                    drift={pendingSaveDrift}
                    onConfirm={handleSaveDriftConfirm}
                    onCancel={handleSaveDriftCancel}
                />
            )}
        </div>
    );
}

function countConnections(c: BotConfig): number {
    return (
        c.connect.httpServers.length +
        c.connect.httpSseServers.length +
        c.connect.httpClients.length +
        c.connect.websocketServers.length +
        c.connect.websocketClients.length
    );
}

function ConnectionCountBadge({ count }: { count: number }) {
    if (count === 0) return null;
    return (
        <span className="ml-1.5 inline-flex h-4 min-w-[16px] items-center justify-center rounded-pill bg-brand-soft px-1 text-2xs font-medium text-brand">
            {count}
        </span>
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
                        <ActionMotionIcon
                            icon={AlertCircle}
                            size={12}
                            strokeWidth={2.4}
                            motion={infoToneMotion('warning')}
                            className="text-warning"
                        />
                        <span className="text-warning">未保存</span>
                    </>
                ) : (
                    <>
                        <ActionMotionIcon
                            icon={Check}
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
                            <ActionMotionIcon icon={Save} size={13} strokeWidth={2.2} />
                            <span>保存</span>
                        </>
                    )}
                </Button>
            </div>
        </div>
    );
}
