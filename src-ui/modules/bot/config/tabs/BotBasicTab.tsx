import React, { useState } from 'react';
import {
 Field,
 Input,
 Checkbox,
 Dropdown,
 Option,
 Text,
 Button,
 Radio,
 RadioGroup,
 Dialog,
 DialogSurface,
 DialogTitle,
 DialogBody,
 DialogContent,
 DialogActions,
} from '@fluentui/react-components';
import { invoke } from '@tauri-apps/api/core';
import { BotBasicConfig } from '../../../../core/ipc/generated/BotBasicConfig';
import { BackendType } from '../../../../core/ipc/generated/BackendType';
import { TimeUnit } from '../../../../core/ipc/generated/TimeUnit';
import { SnowLumaStartMode } from '../../../../core/ipc/generated/SnowLumaStartMode';
import { isTauri } from '../../../../core/ipc/client';

interface BotBasicTabProps {
 data: BotBasicConfig;
 onChange: (updated: Partial<BotBasicConfig>) => void;
 isEditMode: boolean;
}

interface QQProcessInfo {
 pid: number;
 name: string;
 started_at: number;
 command_line: string;
}

export const BotBasicTab: React.FC<BotBasicTabProps> = ({
 data,
 onChange,
 isEditMode,
}) => {
 const handleRestartScheduleChange = (key: keyof BotBasicConfig['autoRestartSchedule'], value: any) => {
 onChange({
 autoRestartSchedule: {
 ...data.autoRestartSchedule,
 [key]: value,
 },
 });
 };

 const timeUnits: { value: TimeUnit; label: string }[] = [
 { value: 'm', label: '分钟 (Minutes)' },
 { value: 'h', label: '小时 (Hours)' },
 { value: 'd', label: '天 (Days)' },
 { value: 'mon', label: '月 (Months)' },
 { value: 'year', label: '年 (Years)' },
 ];

 const backendTypes: { value: BackendType; label: string }[] = [
 { value: 'napcat', label: 'NapCat (本地/标准生态)' },
 { value: 'snowluma', label: 'SnowLuma (轻量/高并发底座)' },
 ];

 // SnowLuma 启动模式：从 data.snowluma_start_mode 推导。
 // null/undefined → cold_start 默认；HotStart{ attach_pid } → hot_start。
 const snowlumaMode: 'cold_start' | 'hot_start' =
 data.snowluma_start_mode?.mode === 'hot_start' ? 'hot_start' : 'cold_start';
 const snowlumaAttachPid: number | null =
 data.snowluma_start_mode?.mode === 'hot_start'
 ? data.snowluma_start_mode.attach_pid
 : null;

 const handleSnowLumaModeChange = (next: 'cold_start' | 'hot_start') => {
 if (next === 'cold_start') {
 // ColdStart：清掉 attach_pid 字段，写 null 让后端按默认（ColdStart）解析。
 const value: SnowLumaStartMode = { mode: 'cold_start' };
 onChange({ snowluma_start_mode: value });
 } else {
 // HotStart：保留旧 PID（若存在），否则用 0 占位（保存前会校验）。
 const value: SnowLumaStartMode = {
 mode: 'hot_start',
 attach_pid: snowlumaAttachPid ?? 0,
 };
 onChange({ snowluma_start_mode: value });
 }
 };

 const handleAttachPidChange = (pidNum: number) => {
 const value: SnowLumaStartMode = { mode: 'hot_start', attach_pid: pidNum };
 onChange({ snowluma_start_mode: value });
 };

 const [pickerOpen, setPickerOpen] = useState(false);
 const [pickerLoading, setPickerLoading] = useState(false);
 const [pickerProcesses, setPickerProcesses] = useState<QQProcessInfo[]>([]);
 const [pickerError, setPickerError] = useState<string | null>(null);

 const openProcessPicker = async => {
 setPickerOpen(true);
 setPickerLoading(true);
 setPickerError(null);
 setPickerProcesses([]);
 try {
 if (isTauri) {
 const result = await invoke<QQProcessInfo[]>('list_qq_processes');
 setPickerProcesses(result);
 } else {
 // Web 预览 mock
 setPickerProcesses([
 { pid: 12345, name: 'QQ.exe', started_at: 0, command_line: '' },
 { pid: 23456, name: 'QQ.exe', started_at: 0, command_line: '' },
 ]);
 }
 } catch (err) {
 setPickerError(`列出 QQ 进程失败: ${String(err)}`);
 } finally {
 setPickerLoading(false);
 }
 };

 const choosePid = (pid: number) => {
 handleAttachPidChange(pid);
 setPickerOpen(false);
 };

 return (
 <div style={{ display: 'flex', flexDirection: 'column', gap: '16px', maxWidth: '500px', padding: '12px 4px' }}>
 <div>
 <Text weight="semibold" size={300}>账号基本信息</Text>
 </div>

 <Field label="账号 (QQ ID)" required hint={isEditMode ? "编辑模式下 QQID 不可更改" : "请输入要托管的纯数字 QQ 账号"}>
 <Input
 type="number"
 value={data.QQID === 0 ? '' : String(data.QQID)}
 onChange={(_, val) => onChange({ QQID: Number(val.value) })}
 disabled={isEditMode}
 placeholder="例如: 10001"
 style={{ width: '100%' }}
 />
 </Field>

 <Field label="实例名称 (Instance Name)" required hint="用于在控制台中唯一识别此 Bot 实例">
 <Input
 value={data.name}
 onChange={(_, val) => onChange({ name: val.value })}
 placeholder="例如: Bot-01"
 style={{ width: '100%' }}
 />
 </Field>

 <Field label="底座适配类型 (Backend Type)">
 <Dropdown
 value={backendTypes.find(t => t.value === data.backend_type)?.label || data.backend_type}
 selectedOptions={[data.backend_type]}
 onOptionSelect={(_, val) => onChange({ backend_type: val.optionValue as BackendType })}
 style={{ width: '100%' }}
 >
 {backendTypes.map((type) => (
 <Option key={type.value} value={type.value}>
 {type.label}
 </Option>
 ))}
 </Dropdown>
 </Field>

 {data.backend_type === 'snowluma' && (
 <div style={{ display: 'flex', flexDirection: 'column', gap: '8px', borderLeft: '2px solid var(--ndf-accent-subtle, #ddd)', paddingLeft: '12px' }}>
 <Text weight="semibold" size={200}>SnowLuma 启动模式</Text>
 <RadioGroup
 value={snowlumaMode}
 onChange={(_, val) => handleSnowLumaModeChange(val.value as 'cold_start' | 'hot_start')}
 >
 <Radio value="cold_start" label="COLD - 由本程序启动一份新的 QQ.exe（推荐）" />
 <Radio value="hot_start" label="HOT - 附加到已存在的 QQ.exe（保留人工登录会话）" />
 </RadioGroup>

 {snowlumaMode === 'hot_start' && (
 <div style={{ display: 'flex', gap: '8px', alignItems: 'flex-end' }}>
 <Field label="目标 QQ.exe PID" style={{ flex: 1 }} hint="保存前请确保该 PID 在系统进程列表中存在">
 <Input
 type="number"
 value={snowlumaAttachPid && snowlumaAttachPid > 0 ? String(snowlumaAttachPid) : ''}
 onChange={(_, val) => handleAttachPidChange(Number(val.value) || 0)}
 placeholder="例如: 12345"
 style={{ width: '100%' }}
 />
 </Field>
 <Button onClick={openProcessPicker} appearance="secondary">
 列出 QQ 进程…
 </Button>
 </div>
 )}
 </div>
 )}

 <Field label="运行时目标 (Runtime Target)" hint="执行 Bot 引擎的宿主平台类型">
 <Dropdown
 value={data.runtime_target}
 selectedOptions={[data.runtime_target]}
 onOptionSelect={(_, val) => onChange({ runtime_target: val.optionValue as string })}
 style={{ width: '100%' }}
 >
 <Option value="local">local (本地物理机运行)</Option>
 <Option value="remote">remote (远程 SSH 主机运行)</Option>
 </Dropdown>
 </Field>

 <Field label="音乐签名接口地址 (Music Sign URL)" hint="发送网易云/QQ 音乐等卡片时所需的签名服务器 Endpoint">
 <Input
 value={data.musicSignUrl}
 onChange={(_, val) => onChange({ musicSignUrl: val.value })}
 placeholder="http://127.0.0.1:8081/sign"
 style={{ width: '100%' }}
 />
 </Field>

 <div style={{ display: 'flex', flexDirection: 'column', gap: '8px', borderTop: '1px solid var(--ndf-border-subtle)', paddingTop: '16px', marginTop: '8px' }}>
 <Text weight="semibold" size={200}>自愈与重启防护 (Self-healing & Restart Rules)</Text>

 <Checkbox
 label="掉线自动重启 (Offline Auto Restart)"
 checked={data.offlineAutoRestart}
 onChange={(_, val) => onChange({ offlineAutoRestart: !!val.checked })}
 />

 <Checkbox
 label="开启定时自动重启任务 (Scheduled Restart)"
 checked={data.autoRestartSchedule.enable}
 onChange={(_, val) => handleRestartScheduleChange('enable', !!val.checked)}
 />

 {data.autoRestartSchedule.enable && (
 <div style={{ display: 'flex', gap: '12px', alignItems: 'flex-start', paddingLeft: '24px', animation: 'fadeIn 0.15s ease-out' }}>
 <Field label="重启间隔 (Restart Interval)" style={{ flex: 1 }}>
 <Input
 type="number"
 value={String(data.autoRestartSchedule.duration)}
 onChange={(_, val) => handleRestartScheduleChange('duration', Number(val.value))}
 placeholder="数字"
 style={{ width: '100%' }}
 />
 </Field>

 <Field label="时间单位 (Unit)" style={{ flex: 1 }}>
 <Dropdown
 value={timeUnits.find(u => u.value === data.autoRestartSchedule.time_unit)?.label || data.autoRestartSchedule.time_unit}
 selectedOptions={[data.autoRestartSchedule.time_unit]}
 onOptionSelect={(_, val) => handleRestartScheduleChange('time_unit', val.optionValue as TimeUnit)}
 style={{ width: '100%' }}
 >
 {timeUnits.map((unit) => (
 <Option key={unit.value} value={unit.value}>
 {unit.label}
 </Option>
 ))}
 </Dropdown>
 </Field>
 </div>
 )}
 </div>

 <Dialog open={pickerOpen} onOpenChange={(_, val) => setPickerOpen(val.open)}>
 <DialogSurface>
 <DialogBody>
 <DialogTitle>选择目标 QQ.exe</DialogTitle>
 <DialogContent>
 {pickerLoading && <Text>加载中…</Text>}
 {pickerError && <Text style={{ color: 'crimson' }}>{pickerError}</Text>}
 {!pickerLoading && !pickerError && pickerProcesses.length === 0 && (
 <Text>未发现正在运行的 QQ.exe，请先手动启动。</Text>
 )}
 {pickerProcesses.length > 0 && (
 <div style={{ display: 'flex', flexDirection: 'column', gap: '4px', maxHeight: '300px', overflowY: 'auto' }}>
 {pickerProcesses.map((p) => (
 <Button
 key={p.pid}
 appearance="subtle"
 style={{ justifyContent: 'flex-start', textAlign: 'left' }}
 onClick={ => choosePid(p.pid)}
 >
 <span>
 <strong>PID {p.pid}</strong> · {p.name}
 {p.command_line && (
 <span style={{ color: 'var(--ndf-fg-secondary, #666)', marginLeft: 8, fontSize: 12 }}>
 {p.command_line}
 </span>
 )}
 </span>
 </Button>
 ))}
 </div>
 )}
 </DialogContent>
 <DialogActions>
 <Button appearance="secondary" onClick={ => setPickerOpen(false)}>
 取消
 </Button>
 </DialogActions>
 </DialogBody>
 </DialogSurface>
 </Dialog>
 </div>
 );
};
