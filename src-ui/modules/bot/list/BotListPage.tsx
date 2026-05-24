import React, { useState, useEffect, useRef } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import {
    Button,
    Spinner,
    MessageBar,
    MessageBarBody,
    Tooltip,
    Text,
    Dialog,
    DialogSurface,
    DialogTitle,
    DialogBody,
    DialogContent,
    DialogActions,
} from '@fluentui/react-components';
import {
    AddRegular,
    ArrowClockwiseRegular,
    CheckmarkCircleRegular,
} from '@fluentui/react-icons';
import { botCommands } from '../../../core/ipc/botCommands';
import { subscribeToEvents } from '../../../core/ipc/events';
import { BotActorSnapshot, NapCatLoginInvalidationReason, DaemonState, SnowLumaLoginState } from '../../../core/ipc/types';
import { BotCard } from './BotCard';
import { BatchToolbar } from './BatchToolbar';
import './BotListPage.css';

interface BotListPageProps {
    onConfigureBot: (botId: string | null) => void;
    onViewLogs: (botId: string) => void;
}

export const BotListPage: React.FC<BotListPageProps> = ({
    onConfigureBot,
    onViewLogs,
}) => {
    const queryClient = useQueryClient();
    const [actionMessage, setActionMessage] = useState<{ type: 'success' | 'error'; text: string } | null>(null);

    // Batch Mode States
    const [isBatchMode, setIsBatchMode] = useState(false);
    const [selectedBotIds, setSelectedBotIds] = useState<Set<string>>(new Set());
    const [isBatchDeleteDialogOpen, setIsBatchDeleteDialogOpen] = useState(false);

    // NapCat WebUI 登录态聚合 state（来自 5 个领域事件）
    const [webuiByBot, setWebuiByBot] = useState<Record<string, { port: number; token: string }>>({});
    const [qrcodeByBot, setQrcodeByBot] = useState<Record<string, string>>({});
    const [onlineByBot, setOnlineByBot] = useState<Record<string, boolean>>({});
    const [invalidatedByBot, setInvalidatedByBot] = useState<Record<string, NapCatLoginInvalidationReason>>({});
    // 失效事件 3s 自动清除定时器引用，按 bot_id 索引
    const invalidationTimersRef = useRef<Record<string, ReturnType<typeof setTimeout>>>({});

    // SnowLuma 系列聚合 state。
    // - daemon 全局态：所有 SL flavor BotCard 共用一份。
    // - injected / uin / loginState：per-Bot 字典。
    const [snowlumaDaemonState, setSnowlumaDaemonState] = useState<DaemonState | null>(null);
    const [snowlumaInjectedByBot, setSnowlumaInjectedByBot] = useState<Record<string, boolean>>({});
    const [snowlumaUinByBot, setSnowlumaUinByBot] = useState<Record<string, string>>({});
    const [snowlumaLoginStateByBot, setSnowlumaLoginStateByBot] = useState<Record<string, SnowLumaLoginState>>({});

    // Bot flavor map：用于决定 BotCard 是否显示 SnowLuma 徽章。
    // 在快照加载后惰性拉取，缓存避免重复 invoke。
    const [flavorByBot, setFlavorByBot] = useState<Record<string, 'napcat' | 'snowluma'>>({});

    // Query bot snapshots
    const { data: botSnapshots = [], isLoading, error, refetch } = useQuery<BotActorSnapshot[], Error>({
        queryKey: ['botSnapshots'],
        queryFn: botCommands.listBotSnapshots,
    });

    // 异步拉 bot config 的 backend_type，缓存到 flavorByBot map。
    // 仅对未缓存的 bot 触发 invoke，避免 N+1 抖动。
    useEffect(() => {
        let cancelled = false;
        const todo = botSnapshots
            .map((b) => b.bot_id)
            .filter((id) => !(id in flavorByBot));
        if (todo.length === 0) return;
        (async () => {
            const updates: Record<string, 'napcat' | 'snowluma'> = {};
            for (const id of todo) {
                try {
                    const cfg = await botCommands.getBotConfig(id);
                    if (cancelled) return;
                    if (cfg) {
                        updates[id] = cfg.bot.backend_type;
                    }
                } catch (err) {
                    console.warn(`flavor lookup failed for ${id}:`, err);
                }
            }
            if (!cancelled && Object.keys(updates).length > 0) {
                setFlavorByBot((prev) => ({ ...prev, ...updates }));
            }
        })();
        return () => {
            cancelled = true;
        };
        // 仅当 botSnapshots 列表的 bot_id 集合变更时才需重跑。
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [botSnapshots.map((b) => b.bot_id).join(',')]);

    // Listen for Tauri events and merge update cache incrementally
    useEffect(() => {
        let unsubscribe: (() => void) | undefined;
        const setup = async () => {
            unsubscribe = await subscribeToEvents((event) => {
                switch (event.kind) {
                    case 'bot_state_changed': {
                        console.log('Bot state changed event received:', event);
                        queryClient.setQueryData<BotActorSnapshot[]>(['botSnapshots'], (old) => {
                            if (!old) return old;
                            return old.map((snap) => {
                                if (snap.bot_id === event.snapshot.bot_id) {
                                    return event.snapshot;
                                }
                                return snap;
                            });
                        });
                        break;
                    }
                    case 'napcat_webui_available': {
                        setWebuiByBot((prev) => ({
                            ...prev,
                            [event.bot_id]: { port: event.port, token: event.token },
                        }));
                        break;
                    }
                    case 'napcat_login_qrcode': {
                        setQrcodeByBot((prev) => ({
                            ...prev,
                            [event.bot_id]: event.qrcode_url,
                        }));
                        break;
                    }
                    case 'napcat_login_qrcode_removed': {
                        setQrcodeByBot((prev) => {
                            if (!(event.bot_id in prev)) return prev;
                            const next = { ...prev };
                            delete next[event.bot_id];
                            return next;
                        });
                        break;
                    }
                    case 'napcat_login_online': {
                        setOnlineByBot((prev) => ({
                            ...prev,
                            [event.bot_id]: event.online,
                        }));
                        break;
                    }
                    case 'napcat_login_invalidated': {
                        const botId: string = event.bot_id;
                        setInvalidatedByBot((prev) => ({
                            ...prev,
                            [botId]: event.reason,
                        }));
                        // 清掉旧定时器（如果有），避免覆盖时窗错位
                        const prevTimer = invalidationTimersRef.current[botId];
                        if (prevTimer) {
                            clearTimeout(prevTimer);
                        }
                        invalidationTimersRef.current[botId] = setTimeout(() => {
                            setInvalidatedByBot((prev) => {
                                if (!(botId in prev)) return prev;
                                const next = { ...prev };
                                delete next[botId];
                                return next;
                            });
                            delete invalidationTimersRef.current[botId];
                        }, 3000);
                        break;
                    }
                    case 'snowluma_daemon_state_changed': {
                        setSnowlumaDaemonState(event.state);
                        // daemon Crashed 时清空 per-Bot 状态：所有 SL Bot 失效
                        if (event.state === 'crashed') {
                            setSnowlumaInjectedByBot({});
                            setSnowlumaLoginStateByBot({});
                        }
                        break;
                    }
                    case 'snowluma_bot_injected': {
                        setSnowlumaInjectedByBot((prev) => ({
                            ...prev,
                            [event.bot_id]: true,
                        }));
                        break;
                    }
                    case 'snowluma_uin_detected': {
                        setSnowlumaUinByBot((prev) => ({
                            ...prev,
                            [event.bot_id]: event.uin,
                        }));
                        break;
                    }
                    case 'snowluma_login_state_changed': {
                        setSnowlumaLoginStateByBot((prev) => ({
                            ...prev,
                            [event.bot_id]: event.state,
                        }));
                        break;
                    }
                    case 'snowluma_pid_set_changed': {
                        // 前端当前不展示 PID 集合，直接忽略；后端事件保留供未来诊断面板用。
                        break;
                    }
                    case 'snowluma_daemon_log': {
                        // SnowLuma daemon 共享 stdout 行：转发到 BotLogPage 由其消费
                        // 此处不需要全局聚合（per-Bot 日志面板自行订阅 events）。
                        break;
                    }
                    default:
                        break;
                }
            });
        };
        setup();
        return () => {
            if (unsubscribe) unsubscribe();
            // 清理所有挂起的失效定时器
            const timers = invalidationTimersRef.current;
            for (const id of Object.keys(timers)) {
                clearTimeout(timers[id]);
                delete timers[id];
            }
        };
    }, [queryClient]);

    // Mutations
    const startMutation = useMutation({
        mutationFn: botCommands.startBot,
        onSuccess: (updatedSnap) => {
            queryClient.setQueryData<BotActorSnapshot[]>(['botSnapshots'], (old) => {
                if (!old) return old;
                return old.map((snap) => snap.bot_id === updatedSnap.bot_id ? updatedSnap : snap);
            });
            setActionMessage({ type: 'success', text: `已发送启动指令给 Bot: ${updatedSnap.bot_id}` });
        },
        onError: (err: any) => {
            setActionMessage({ type: 'error', text: `启动失败: ${err.message || err}` });
        },
    });

    const stopMutation = useMutation({
        mutationFn: botCommands.stopBot,
        onSuccess: (updatedSnap) => {
            queryClient.setQueryData<BotActorSnapshot[]>(['botSnapshots'], (old) => {
                if (!old) return old;
                return old.map((snap) => snap.bot_id === updatedSnap.bot_id ? updatedSnap : snap);
            });
            setActionMessage({ type: 'success', text: `已发送停止指令给 Bot: ${updatedSnap.bot_id}` });
        },
        onError: (err: any) => {
            setActionMessage({ type: 'error', text: `停止失败: ${err.message || err}` });
        },
    });

    const batchStartMutation = useMutation({
        mutationFn: botCommands.batchStartBots,
        onSuccess: (res) => {
            refetch();
            const sCount = res.succeeded.length;
            const fCount = res.failed.length;
            const failDetail = fCount > 0 ? ` 失败项: ${res.failed.map(([id, reason]) => `${id}(${reason})`).join(', ')}` : '';
            setActionMessage({
                type: fCount === 0 ? 'success' : 'error',
                text: `批量启动指令执行完毕。成功: ${sCount} 个, 失败: ${fCount} 个。${failDetail}`,
            });
            setIsBatchMode(false);
            setSelectedBotIds(new Set());
        },
        onError: (err: any) => {
            setActionMessage({ type: 'error', text: `批量启动失败: ${err.message || err}` });
        },
    });

    const batchStopMutation = useMutation({
        mutationFn: botCommands.batchStopBots,
        onSuccess: (res) => {
            refetch();
            const sCount = res.succeeded.length;
            const fCount = res.failed.length;
            const failDetail = fCount > 0 ? ` 失败项: ${res.failed.map(([id, reason]) => `${id}(${reason})`).join(', ')}` : '';
            setActionMessage({
                type: fCount === 0 ? 'success' : 'error',
                text: `批量停止指令执行完毕。成功: ${sCount} 个, 失败: ${fCount} 个。${failDetail}`,
            });
            setIsBatchMode(false);
            setSelectedBotIds(new Set());
        },
        onError: (err: any) => {
            setActionMessage({ type: 'error', text: `批量停止失败: ${err.message || err}` });
        },
    });

    const batchDeleteMutation = useMutation({
        mutationFn: botCommands.batchDeleteBots,
        onSuccess: (res) => {
            refetch();
            const sCount = res.succeeded.length;
            const fCount = res.failed.length;
            const failDetail = fCount > 0 ? ` 失败项: ${res.failed.map(([id, reason]) => `${id}(${reason})`).join(', ')}` : '';
            setActionMessage({
                type: fCount === 0 ? 'success' : 'error',
                text: `批量删除执行完毕。成功删除: ${sCount} 个实例, 失败: ${fCount} 个。${failDetail}`,
            });
            setIsBatchMode(false);
            setSelectedBotIds(new Set());
            setIsBatchDeleteDialogOpen(false);
        },
        onError: (err: any) => {
            setActionMessage({ type: 'error', text: `批量删除失败: ${err.message || err}` });
            setIsBatchDeleteDialogOpen(false);
        },
    });

    const handleStartBot = (botId: string) => {
        setActionMessage(null);
        startMutation.mutate(botId);
    };

    const handleStopBot = (botId: string) => {
        setActionMessage(null);
        stopMutation.mutate(botId);
    };

    const handleToggleSelect = (botId: string) => {
        setSelectedBotIds((prev) => {
            const next = new Set(prev);
            if (next.has(botId)) {
                next.delete(botId);
            } else {
                next.add(botId);
            }
            return next;
        });
    };

    const handleBatchStart = () => {
        if (selectedBotIds.size === 0) return;
        setActionMessage(null);
        batchStartMutation.mutate(Array.from(selectedBotIds));
    };

    const handleBatchStop = () => {
        if (selectedBotIds.size === 0) return;
        setActionMessage(null);
        batchStopMutation.mutate(Array.from(selectedBotIds));
    };

    const handleBatchDelete = () => {
        if (selectedBotIds.size === 0) return;
        setActionMessage(null);
        batchDeleteMutation.mutate(Array.from(selectedBotIds));
    };

    const toggleBatchMode = () => {
        setIsBatchMode((prev) => {
            const next = !prev;
            if (!next) {
                setSelectedBotIds(new Set());
            }
            return next;
        });
    };

    return (
        <div className="ndf-bot-panel-container">
            {/* 1. Header Text and Action group */}
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start', marginBottom: '20px' }}>
                <div>
                    <Text size={500} weight="semibold" style={{ color: 'var(--colorNeutralForeground1)' }}>
                        Bot 实例管理 (Bot Instance Manager)
                    </Text>
                    <Text size={100} block style={{ color: 'var(--colorNeutralForeground4)', marginTop: '4px' }}>
                        管理本地与远程的 Bot 实例、维护各自的数据路由以及底层引擎的生命周期。
                    </Text>
                </div>
            </div>

            {/* 2. Messages & Alerts Area */}
            {actionMessage && (
                <MessageBar
                    intent={actionMessage.type}
                    style={{ marginBottom: '16px' }}
                >
                    <MessageBarBody>{actionMessage.text}</MessageBarBody>
                </MessageBar>
            )}

            {/* 3. Loading & Error States */}
            {isLoading ? (
                <div style={{ display: 'flex', flexDirection: 'column', gap: '10px', alignItems: 'center', padding: '100px 0' }}>
                    <Spinner size="large" />
                    <Text size={200} style={{ color: 'var(--colorNeutralForeground4)' }}>正在检索 Bot 实例快照数据...</Text>
                </div>
            ) : error ? (
                <div style={{ padding: '24px', backgroundColor: 'var(--colorPaletteRedBackground1)', border: '1px solid var(--colorPaletteRedBorder1)', borderRadius: '6px' }}>
                    <Text size={300} weight="semibold" style={{ color: 'var(--colorPaletteRedForeground1)' }}>检索数据失败</Text>
                    <Text size={200} block style={{ marginTop: '4px', color: 'var(--colorPaletteRedForeground1)' }}>{error.message}</Text>
                </div>
            ) : botSnapshots.length === 0 ? (
                <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', padding: '80px 0', border: '1px dashed var(--ndf-border-subtle)', borderRadius: '12px', backgroundColor: 'var(--ndf-bg-card)' }}>
                    <Text size={300} weight="semibold" style={{ color: 'var(--colorNeutralForeground3)' }}>暂无 Bot 实例快照</Text>
                    <Text size={100} block style={{ color: 'var(--colorNeutralForeground4)', marginTop: '4px', marginBottom: '16px' }}>
                        你尚未创建任何 Bot 实例配置文件，点击右下角加号按钮来初始化首个配置。
                    </Text>
                    <Button icon={<AddRegular />} appearance="primary" onClick={() => onConfigureBot(null)}>
                        创建第一个实例
                    </Button>
                </div>
            ) : (
                /* 4. Grid Flow Panels of BotCards */
                <div className="ndf-bot-flow-grid">
                    {botSnapshots.map((bot) => {
                        const webui = webuiByBot[bot.bot_id];
                        const isSnowLuma = flavorByBot[bot.bot_id] === 'snowluma';
                        return (
                            <BotCard
                                key={bot.bot_id}
                                bot={bot}
                                onStart={handleStartBot}
                                onStop={handleStopBot}
                                onConfigure={onConfigureBot}
                                onViewLogs={onViewLogs}
                                isBatchMode={isBatchMode}
                                isSelected={selectedBotIds.has(bot.bot_id)}
                                onToggleSelect={handleToggleSelect}
                                qrcodeUrl={qrcodeByBot[bot.bot_id] ?? null}
                                isOnline={
                                    bot.bot_id in onlineByBot ? onlineByBot[bot.bot_id] : null
                                }
                                invalidationReason={invalidatedByBot[bot.bot_id] ?? null}
                                webuiPort={webui ? webui.port : null}
                                webuiToken={webui ? webui.token : null}
                                isSnowLuma={isSnowLuma}
                                snowlumaDaemonState={snowlumaDaemonState}
                                snowlumaInjected={snowlumaInjectedByBot[bot.bot_id] ?? false}
                                snowlumaUin={snowlumaUinByBot[bot.bot_id] ?? null}
                                snowlumaLoginState={snowlumaLoginStateByBot[bot.bot_id] ?? null}
                            />
                        );
                    })}
                </div>
            )}

            {/* 5. Circular floating action triggers inside Canvas Viewport */}
            {!isBatchMode && (
                <div className="ndf-floating-group">
                    <Tooltip content="批量管理 (Batch Actions)" relationship="label">
                        <Button
                            className="ndf-circle-btn"
                            icon={<CheckmarkCircleRegular style={{ fontSize: '18px', color: 'var(--colorNeutralForeground2)' }} />}
                            onClick={toggleBatchMode}
                        />
                    </Tooltip>

                    <Tooltip content="刷新状态 (Manual Refresh)" relationship="label">
                        <Button
                            className="ndf-circle-btn"
                            icon={<ArrowClockwiseRegular style={{ fontSize: '18px', color: 'var(--colorNeutralForeground2)' }} />}
                            onClick={() => refetch()}
                        />
                    </Tooltip>

                    <Tooltip content="新增 Bot 实例 (Create Config)" relationship="label">
                        <Button
                            className="ndf-circle-btn ndf-primary-circle"
                            icon={<AddRegular style={{ fontSize: '20px', color: '#ffffff' }} />}
                            onClick={() => onConfigureBot(null)}
                        />
                    </Tooltip>
                </div>
            )}

            {/* 6. Active Batch Management overlay bar */}
            <BatchToolbar
                selectedCount={selectedBotIds.size}
                onBatchStart={handleBatchStart}
                onBatchStop={handleBatchStop}
                onBatchDelete={() => setIsBatchDeleteDialogOpen(true)}
                onCancel={toggleBatchMode}
                isLoading={batchStartMutation.isPending || batchStopMutation.isPending || batchDeleteMutation.isPending}
            />

            {/* 7. Batch Delete Confirmation Dialog */}
            <Dialog open={isBatchDeleteDialogOpen} onOpenChange={(_, data) => setIsBatchDeleteDialogOpen(data.open)}>
                <DialogSurface style={{ maxWidth: '400px' }}>
                    <DialogBody>
                        <DialogTitle>确认批量删除选中的实例？</DialogTitle>
                        <DialogContent style={{ marginTop: '10px' }}>
                            <Text>
                                你即将删除选中的 <Text weight="bold" style={{ color: '#bc2f32' }}>{selectedBotIds.size}</Text> 个 Bot 实例的全部配置文件与数据项。
                            </Text>
                            <Text block style={{ color: 'var(--colorNeutralForeground4)', marginTop: '8px', fontSize: '12px' }}>
                                如果其中有任何正在运行的 Bot，系统会先自动将其停止后再彻底删除。此操作不可撤销！
                            </Text>
                        </DialogContent>
                        <DialogActions style={{ marginTop: '16px' }}>
                            <Button appearance="secondary" onClick={() => setIsBatchDeleteDialogOpen(false)}>
                                取消
                            </Button>
                            <Button
                                appearance="primary"
                                style={{ backgroundColor: '#bc2f32', color: '#ffffff', border: 'none' }}
                                onClick={handleBatchDelete}
                                disabled={batchDeleteMutation.isPending}
                            >
                                彻底删除
                            </Button>
                        </DialogActions>
                    </DialogBody>
                </DialogSurface>
            </Dialog>
        </div>
    );
};
