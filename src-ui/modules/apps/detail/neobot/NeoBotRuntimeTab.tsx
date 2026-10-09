import { Button, Checkbox } from '../../../../shared/ui';
import { asRecord } from '../../../../core/domain/apps/neobotPanel';
import { records, text } from '../../../../core/domain/apps/neobotWorkspace';
import { usePanelJson } from '../../../../hooks/apps/useNeoBotPanel';
import { useNeoBotAction } from '../../../../hooks/apps/useNeoBotAction';
import {
    ConfirmAction,
    ObjectView,
    PanelPage,
    PanelSection,
    RecordTable,
    type NeoBotPageProps,
} from './workspaceParts';

export function NeoBotRuntimeTab({ instanceId, onGoTab }: NeoBotPageProps) {
    const power = usePanelJson(instanceId, 'power', '/api/admin/power', asRecord, true, 3000);
    const system = usePanelJson(instanceId, 'system', '/api/system', asRecord, true, 10000);
    const services = usePanelJson(instanceId, 'services', '/api/services', asRecord);
    const tasks = usePanelJson(instanceId, 'tasks', '/api/tasks', asRecord, true, 10000);
    const bot = usePanelJson(instanceId, 'botDetail', '/api/bot/detail', asRecord);
    const bots = usePanelJson(instanceId, 'bots', '/api/bots', (raw) =>
        Array.isArray(raw) ? { items: raw } : asRecord(raw),
    );
    const action = useNeoBotAction(instanceId);
    return (
        <div className="flex flex-col gap-4">
            <PanelPage query={power} onGoTab={onGoTab}>
                {(data) => (
                    <PanelSection title="运行状态">
                        <ObjectView data={data} />
                        <div className="mt-4 flex flex-wrap gap-2">
                            <ConfirmAction
                                label="进入待机"
                                description="停止回复与记忆处理，保留面板和核心服务。"
                                disabled={
                                    action.isPending ||
                                    data.transition === true ||
                                    data.standby === true ||
                                    data.available === false
                                }
                                onConfirm={() =>
                                    void action.run({
                                        path: '/api/admin/standby',
                                        body: { reason: 'desktop' },
                                    })
                                }
                            />
                            <Button
                                size="sm"
                                variant="primary"
                                disabled={
                                    action.isPending ||
                                    data.transition === true ||
                                    data.standby !== true ||
                                    data.available === false
                                }
                                onClick={() =>
                                    void action.run({
                                        path: '/api/admin/resume',
                                        body: { reason: 'desktop' },
                                    })
                                }
                            >
                                恢复运行
                            </Button>
                            <ConfirmAction
                                label="软重启运行"
                                description="重新构建 Bot 运行时，处理中的回复会中断。面板进程继续保留。"
                                disabled={
                                    action.isPending ||
                                    data.transition === true ||
                                    data.available === false
                                }
                                onConfirm={() =>
                                    void action.run({
                                        path: '/api/admin/reboot',
                                        body: { reason: 'desktop' },
                                    })
                                }
                            />
                            <ConfirmAction
                                label="重启进程"
                                description="重启 NeoBot，面板会短暂断开，代码与启动期配置随后生效。"
                                disabled={action.isPending || data.transition === true}
                                onConfirm={() => void action.run({ path: '/api/admin/restart' })}
                            />
                        </div>
                        <Checkbox
                            className="mt-4"
                            label="待机时保持 OneBot 连接"
                            checked={data.connect_onebot === true}
                            disabled={
                                action.isPending ||
                                data.transition === true ||
                                data.available === false
                            }
                            onCheckedChange={(enabled) =>
                                void action.run({
                                    path: '/api/admin/standby/onebot',
                                    body: { enabled: enabled === true },
                                })
                            }
                        />
                    </PanelSection>
                )}
            </PanelPage>
            <PanelPage query={system} onGoTab={onGoTab}>
                {(data) => (
                    <PanelSection title="系统资源">
                        <ObjectView data={data} />
                    </PanelSection>
                )}
            </PanelPage>
            <PanelPage query={bot} onGoTab={onGoTab}>
                {(data) => (
                    <PanelSection title="机器人详情">
                        <ObjectView data={data} />
                    </PanelSection>
                )}
            </PanelPage>
            <PanelPage query={bots} onGoTab={onGoTab}>
                {(data) => (
                    <PanelSection title="机器人与连接">
                        <RecordTable
                            items={records(data.items).map((bot) => ({
                                ...bot,
                                nickname: bot.nickname || bot.name,
                            }))}
                            columns={[
                                ['user_id', 'QQ'],
                                ['nickname', '昵称'],
                                ['platform', '平台'],
                                ['status', '状态'],
                                ['latency_ms', '延迟 ms'],
                            ]}
                        />
                    </PanelSection>
                )}
            </PanelPage>
            <PanelPage query={services} onGoTab={onGoTab}>
                {(data) => (
                    <PanelSection title="服务">
                        <RecordTable
                            items={records(data.items)}
                            columns={[
                                ['name', '服务'],
                                ['description', '说明'],
                                ['available', '可用'],
                            ]}
                        />
                    </PanelSection>
                )}
            </PanelPage>
            <PanelPage query={tasks} onGoTab={onGoTab}>
                {(data) => (
                    <PanelSection
                        title="后台任务"
                        actions={
                            <Button size="sm" variant="ghost" onClick={() => onGoTab('scheduled')}>
                                管理定时任务
                            </Button>
                        }
                    >
                        <RecordTable
                            items={[...records(data.background), ...records(data.scheduled)]}
                            columns={[
                                ['name', '任务'],
                                ['task_id', 'ID'],
                                ['status', '状态'],
                                ['pipeline_key', '对话流'],
                                ['next_run', '下次执行'],
                            ]}
                        />
                        {text(data.error) && (
                            <p className="mt-3 text-xs text-warning">{text(data.error)}</p>
                        )}
                    </PanelSection>
                )}
            </PanelPage>
        </div>
    );
}
