// 演示远端 MachineView：假数据 + 真卡壳，零 SSH。
// 显式依赖：NC → QQ；SL → QQ + Node + noVNC。

import type { ComponentInfo } from '../../ipc/types';
import type { MachineComponentRow, MachineView } from '../components/types';
import { DEMO_REMOTE_HOST_ID } from './tourIds';

function info(
    id: ComponentInfo['id'],
    display_name: string,
    description: string,
    category: ComponentInfo['category'],
): ComponentInfo {
    return {
        id,
        display_name,
        description,
        repo_url: null,
        supported_targets: [],
        category,
    };
}

function notInstalled(component: ComponentInfo): MachineComponentRow {
    return {
        info: component,
        status: { state: 'not_installed' },
    };
}

/** 演示用 Linux 远端：按显式依赖展示，全部未安装。 */
export function buildDemoRemoteMachine(): MachineView {
    return {
        host: {
            host_id: DEMO_REMOTE_HOST_ID,
            display_name: '演示远端（Linux）',
            os: 'linux',
            locality: 'remote',
            state: 'connected',
        },
        framework: [
            notInstalled(
                info(
                    'napcat',
                    'NapCat',
                    '远端协议端，依赖 QQ',
                    'framework',
                ),
            ),
            notInstalled(
                info(
                    'snowluma',
                    'SnowLuma',
                    '远端带 QQ 窗口，依赖 QQ + Node + noVNC',
                    'framework',
                ),
            ),
        ],
        runtimeDep: [
            notInstalled(
                info('qq', 'QQ', 'NC / SL 都用（Linux QQ）', 'runtime_dep'),
            ),
            notInstalled(
                info(
                    'nodejs',
                    'Node.js',
                    '远端 SnowLuma 需要',
                    'runtime_dep',
                ),
            ),
            notInstalled(
                info(
                    'novnc',
                    'noVNC',
                    '远端 SnowLuma 看 QQ 窗口用',
                    'runtime_dep',
                ),
            ),
        ],
        selfApp: [],
    };
}
