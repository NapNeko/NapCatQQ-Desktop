// 实例表的唯一所有者：整个 app-framework mock 的增删改都收敛到这里，
// 其余模块 import 同一数组就地增删；改写走 publish（同址替换条目）。
import type { AppInstance } from '../../types';
import { emitMockEvent } from '../events.mock';

export const instances: AppInstance[] = [
    {
        id: 'k1a2b3c4',
        framework_id: 'karin',
        display_name: 'Karin · 本机',
        placement: 'local_native',
        host_id: 'local',
        install_dir: 'D:/NapCatQQ/apps/karin/k1a2b3c4',
        port: 7777,
        state: 'running',
        link: {
            bot_id: '10001',
            mode: 'reverse_ws',
            connection_name: 'ncd-app:k1a2b3c4',
            linked_at_ms: Date.now() - 3_600_000,
        },
        installed_version: '1.17.0',
        created_at_ms: Date.now() - 86_400_000,
        install_renderer: true,
        origin: 'created',
        auto_start: true,
    },
    {
        id: 'd5e6f7a8',
        framework_id: 'karin',
        display_name: 'Karin · production',
        placement: 'remote_native',
        host_id: 'remote:production',
        install_dir: '/home/ubuntu/ncd/apps/karin/d5e6f7a8',
        port: 7777,
        state: 'not_installed',
        created_at_ms: Date.now() - 600_000,
        install_renderer: true,
        origin: 'created',
        auto_start: true,
    },
    {
        id: 'n9b8c7d6',
        framework_id: 'nonebot2',
        display_name: '荒境修仙',
        placement: 'remote_native',
        host_id: 'remote:production',
        install_dir: '/root/game-qqbot/bot-xiuxian',
        port: 13120,
        state: 'running',
        link: {
            bot_id: '10001',
            mode: 'reverse_ws',
            connection_name: 'ncd-app:n9b8c7d6',
            linked_at_ms: Date.now() - 1_800_000,
        },
        installed_version: '2.5.1',
        created_at_ms: Date.now() - 7_200_000,
        install_renderer: false,
        origin: 'imported',
        auto_start: true,
    },
    {
        id: 'ab12cd34',
        framework_id: 'astrbot',
        display_name: 'AstrBot · 本机',
        placement: 'local_native',
        host_id: 'local',
        install_dir: 'D:/NapCatQQ/apps/astrbot/ab12cd34',
        port: 6199,
        state: 'running',
        created_at_ms: Date.now() - 3_600_000,
        install_renderer: false,
        origin: 'created',
        auto_start: true,
    },
    {
        id: 'mb56ef78',
        framework_id: 'maibot',
        display_name: '麦麦 · 本机',
        placement: 'local_native',
        host_id: 'local',
        install_dir: 'D:/NapCatQQ/apps/maibot/mb56ef78',
        port: 23001,
        state: 'stopped',
        installed_version: '1.2.5',
        created_at_ms: Date.now() - 1_200_000,
        install_renderer: false,
        origin: 'created',
        auto_start: false,
    },
    {
        id: 'ko90ab12',
        framework_id: 'koishi',
        display_name: 'Koishi · 本机',
        placement: 'local_native',
        host_id: 'local',
        install_dir: 'D:/NapCatQQ/apps/koishi/ko90ab12',
        port: 23140,
        state: 'running',
        installed_version: '4.18.11',
        created_at_ms: Date.now() - 600_000,
        install_renderer: false,
        origin: 'created',
        auto_start: true,
    },
    {
        id: 'nb7c1d20',
        framework_id: 'neobot',
        display_name: 'NeoBot · 本机',
        placement: 'local_native',
        host_id: 'local',
        install_dir: 'D:/NapCatQQ/apps/neobot/nb7c1d20',
        port: 8080,
        state: 'running',
        link: {
            bot_id: '10001',
            mode: 'reverse_ws',
            connection_name: 'ncd-app:nb7c1d20',
            linked_at_ms: Date.now() - 1_200_000,
        },
        installed_version: '1.2.4a1',
        created_at_ms: Date.now() - 3_600_000,
        install_renderer: false,
        origin: 'created',
        auto_start: true,
    },
    {
        id: 'yz90ab12',
        framework_id: 'yunzai',
        display_name: '云崽 · 本机',
        placement: 'local_native',
        host_id: 'local',
        install_dir: 'D:/NapCatQQ/apps/yunzai/yz90ab12',
        port: 2536,
        state: 'stopped',
        installed_version: '3.1.3',
        created_at_ms: Date.now() - 900_000,
        install_renderer: true,
        origin: 'created',
        auto_start: false,
    },
];

/// 假装实例目录里的条款：MaiBot 实例启动前要同意一次，同意过的记在这里
export const mockAcceptedTerms = new Set<string>();
export const MOCK_TERMS_TEXT: Record<string, string> = {
    eula: '# MaiBot最终用户许可协议\n\n**版本：V1.3**\n\n1. 本项目免费开源，禁止倒卖。\n2. 使用本项目产生的内容由使用者自行负责。\n',
    privacy:
        '### MaiBot用户隐私条款\n\n**版本：V1.2**\n\n- 聊天记录只存在你自己的机器上。\n- 默认上报匿名统计，可在配置里关掉。\n',
};

export function publish(instance: AppInstance, reason: string) {
    const at = instances.findIndex((i) => i.id === instance.id);
    if (at >= 0) instances.splice(at, 1, instance);
    emitMockEvent({ kind: 'app_instance_changed', instance, reason });
}

export function require(id: string): AppInstance {
    const found = instances.find((i) => i.id === id);
    if (!found) throw new Error(`应用实例不存在: ${id}`);
    return found;
}

/** 别的 mock（麦麦资源页）按 id 取实例看它在不在跑 */
export function peekMockAppInstance(id: string): AppInstance {
    return require(id);
}
