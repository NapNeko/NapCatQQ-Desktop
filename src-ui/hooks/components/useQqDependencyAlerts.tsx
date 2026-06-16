// 组件页：远端 Linux 上 QQ 系统依赖缺失时推 InfoBar，点击后走任务队列 ensure_dependencies。
// 平时 QQ 卡片不挂「补全依赖」按钮。

import { useEffect, useRef } from 'react';
import type { MachineView } from '../../core/domain/components/types';
import { componentService } from '../../core/services/component.service';
import { pushInfoBar, dismissInfoBar } from '../ui/globalInfoBarStore';

const alertKey = (hostId: string) => `qq-system-deps:${hostId}`;

function isRemoteLinux(machine: MachineView | null): boolean {
    if (!machine) return false;
    return machine.host.os === 'linux' && machine.host.locality === 'remote';
}

export function useQqDependencyAlerts(
    activeMachine: MachineView | null,
    onRepair: (hostId: string) => void,
): void {
    const onRepairRef = useRef(onRepair);
    onRepairRef.current = onRepair;
    const checkedHostsRef = useRef<Set<string>>(new Set());

    useEffect(() => {
        if (!isRemoteLinux(activeMachine)) {
            return;
        }
        const hostId = activeMachine!.host.host_id;
        if (checkedHostsRef.current.has(hostId)) {
            return;
        }
        checkedHostsRef.current.add(hostId);

        let cancelled = false;
        (async () => {
            try {
                const report = await componentService.detectQqDependencies(hostId);
                if (cancelled) return;
                const key = alertKey(hostId);
                if (report.missing.length === 0) {
                    dismissInfoBar(`key:${key}`);
                    return;
                }
                const hostName = activeMachine!.host.display_name;
                pushInfoBar({
                    key,
                    tone: 'warning',
                    title: `QQ 系统依赖未就绪 · ${hostName}`,
                    content: (
                        <span>
                            缺失 {report.missing.length} 个系统包，Bot 可能无法启动 QQ。{' '}
                            <button
                                type="button"
                                className="font-medium text-brand underline"
                                onClick={() => {
                                    dismissInfoBar(`key:${key}`);
                                    onRepairRef.current(hostId);
                                }}
                            >
                                一键修复
                            </button>
                            （进度见「任务」页）
                        </span>
                    ),
                    autoDismissMs: 0,
                });
            } catch (e) {
                console.warn('[useQqDependencyAlerts] detect failed:', e);
            }
        })();

        return () => {
            cancelled = true;
        };
    }, [activeMachine]);
}

export function isQqSystemDependencyError(message: string): boolean {
    return (
        message.includes('缺少系统依赖库')
        || message.toLowerCase().includes('error while loading shared libraries')
        || message.includes('cannot open shared object file')
    );
}