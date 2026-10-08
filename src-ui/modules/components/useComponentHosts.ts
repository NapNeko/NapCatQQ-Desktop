// 组件页主机选择：组件主导矩阵 → 主机主导 + 当前选中主机。
// 从 ComponentsPage.next 外提，页面只消费 machines/activeMachine。

import { useEffect, useMemo, useState } from 'react';
import { groupByHost } from '../../core/domain/components/types';
import { buildDemoRemoteMachine } from '../../core/domain/onboarding/demoRemoteMachine';
import type { ComponentRow, HostInfo, MachineView } from '../../core/domain/components/types';

// componentsHostBridge 的 BridgeState 没有具名导出（hooks 层不对外发布），
// 这里按结构声明消费面， tour 只写这三个字段。
export interface ComponentHostBridgeState {
    preferredHostId: string | null;
    includeDemoRemote: boolean;
    hostSelectionLocked: boolean;
}

export interface ComponentHostsState {
    machines: MachineView[];
    activeHostId: string | null;
    setActiveHostId: (hostId: string) => void;
    activeMachine: MachineView | null;
}

export function useComponentHosts(
    allRows: ComponentRow[],
    hosts: HostInfo[],
    hostBridge: ComponentHostBridgeState,
): ComponentHostsState {
    // 剔掉这台机器一个组件都装不了的空机器。
    const machines = useMemo<MachineView[]>(() => {
        const grouped = groupByHost(allRows, hosts);
        const real = grouped.filter(
            (m) => m.framework.length + m.runtimeDep.length + m.selfApp.length > 0,
        );
        // 框架 tour：注入只读演示远端，不进 servers.json
        if (hostBridge.includeDemoRemote) {
            return [...real, buildDemoRemoteMachine()];
        }
        return real;
    }, [allRows, hosts, hostBridge.includeDemoRemote]);

    // 选中的主机：默认第一台。仅 tour 的 hostSelectionLocked 时强制 preferred；
    // 结束后 locked=false，用户可自由点远端 tab。
    const [activeHostId, setActiveHostId] = useState<string | null>(null);
    useEffect(() => {
        if (machines.length === 0) {
            if (activeHostId !== null) setActiveHostId(null);
            return;
        }
        if (hostBridge.hostSelectionLocked && hostBridge.preferredHostId) {
            const preferred = hostBridge.preferredHostId;
            if (machines.some((m) => m.host.host_id === preferred)) {
                if (activeHostId !== preferred) setActiveHostId(preferred);
                return;
            }
        }
        const stillThere = machines.some((m) => m.host.host_id === activeHostId);
        if (!stillThere) setActiveHostId(machines[0].host.host_id);
    }, [machines, activeHostId, hostBridge.preferredHostId, hostBridge.hostSelectionLocked]);

    const activeMachine = useMemo(
        () => machines.find((m) => m.host.host_id === activeHostId) ?? machines[0] ?? null,
        [machines, activeHostId],
    );

    return { machines, activeHostId, setActiveHostId, activeMachine };
}
