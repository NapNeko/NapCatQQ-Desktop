import { FormSection, Switch, TextField } from '../../../../shared/ui';
import { ExpandPresence } from '../../../../shared/ui/motion/ExpandPresence';
import type { AutoRestartSchedule } from '../../../../core/ipc/generated/domain/AutoRestartSchedule';
import { AutoRestartScheduleEditor } from './AutoRestartScheduleEditor';
import type { IdentityTabProps } from './identityConfig';

export function IdentityRecoverySection({
    data,
    onChange,
}: Pick<IdentityTabProps, 'data' | 'onChange'>) {
    const patchSchedule = (patch: Partial<AutoRestartSchedule>) =>
        onChange({ autoRestartSchedule: { ...data.autoRestartSchedule, ...patch } });
    return (
        <FormSection title="自愈与附加服务" layout="none">
            <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
                <div className="flex items-center rounded-md border border-border-subtle/80 bg-field/35 px-3.5 py-2.5">
                    <div className="w-full">
                        <Switch
                            label="掉线自动重启"
                            hint="进程异常退出或掉线时自动拉起"
                            checked={data.offlineAutoRestart}
                            onCheckedChange={(v) => onChange({ offlineAutoRestart: v })}
                        />
                    </div>
                </div>
                <div className="flex items-center rounded-md border border-border-subtle/80 bg-field/35 px-3.5 py-2.5">
                    <div className="w-full">
                        <Switch
                            label="定时自动重启"
                            hint="按固定间隔或 Cron 计划重启实例"
                            checked={data.autoRestartSchedule.enable}
                            onCheckedChange={(v) => patchSchedule({ enable: v })}
                        />
                    </div>
                </div>
            </div>
            <ExpandPresence visible={data.autoRestartSchedule.enable}>
                <div className="pt-3">
                    <div className="rounded-md border border-border-subtle/70 bg-field/30 px-3.5 py-3">
                        <AutoRestartScheduleEditor
                            schedule={data.autoRestartSchedule}
                            onChange={patchSchedule}
                        />
                    </div>
                </div>
            </ExpandPresence>
            <div className="pt-3">
                <div className="grid grid-cols-1 items-center gap-3 rounded-md border border-border-subtle/80 bg-field/35 px-3.5 py-2.5 sm:grid-cols-2">
                    <div className="min-w-0">
                        <div className="text-sm font-medium text-text">音乐签名接口</div>
                        <div className="text-2xs text-text-tertiary">
                            仅发送音乐卡片时需要，留空则不启用
                        </div>
                    </div>
                    <TextField
                        aria-label="音乐签名接口"
                        value={data.musicSignUrl}
                        onValueChange={(v) => onChange({ musicSignUrl: v })}
                        placeholder="http://127.0.0.1:8081/sign"
                        className="font-mono"
                    />
                </div>
            </div>
        </FormSection>
    );
}
