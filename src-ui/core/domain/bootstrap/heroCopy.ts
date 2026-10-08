// 概览 Hello 卡的文案表：时段闲聊兜底、吉祥物戳台词、实例状态突变时的主动反应句。
// 纯查表 + 随机挑句；「什么时候触发哪一类反应」的状态机在 widgets/HelloCard 的 hook 里。

import type { BotFleetStats } from '../overview/glance';

// 一个实例都没有 / 全停着时，时段闲聊不成立，换成实情。
// 异常数不在这里说，状态行的红色 chip 已经负责。
export function fleetHint(fleet: BotFleetStats): string | null {
    if (fleet.total === 0) return '还没有实例，去实例页建一个。';
    if (fleet.active === 0) return '实例都停着，没人说话。';
    return null;
}

export function pick<T>(lines: readonly T[]): T {
    return lines[Math.floor(Math.random() * lines.length)];
}

export const ALARM_LINES = ['个实例出事了！', '个实例掉线了！', '个实例不对劲！'] as const;
export const RECOVERED_LINES = [
    '都恢复了，虚惊一场。',
    '好了，接着跑。',
    '回来了，我就说没事。',
] as const;
export const ONLINE_LINES = ['上线了，我盯着。', '起来了，交给我。', '连上了，你去忙。'] as const;
export const HALTED_LINES = ['全停了，收工？', '都停了，我也歇会儿。'] as const;

const FLAVOR_QUIPS = [
    '喵。',
    '别戳啦，怕痒。',
    '消息我盯着，你去忙。',
    '记得喝水。',
    '尾巴不能摸。',
    '手别抖，戳偏了。',
    '这里没有彩蛋。',
    '刚才那下有点重。',
    '日志我看了，没什么好看的。',
    '再戳我就装作没看见。',
    '你很闲吗。',
    '我也想歇会儿。',
    '好，你戳，我数着。',
    '再戳要收费了。',
] as const;

// 每次开页换一个起点，免得台词顺序永远一样。
const FLAVOR_START = Math.floor(Math.random() * FLAVOR_QUIPS.length);

// 戳吉祥物的台词：第一句说实情，后面几句是她自己的话。
export function mascotQuips(fleet: BotFleetStats, actionableCount: number): string[] {
    const status =
        actionableCount > 0
            ? `${actionableCount} 个实例不对劲，去看看？`
            : fleet.total === 0
              ? '一个实例都没有，空得慌。'
              : fleet.running > 0
                ? `${fleet.running} 个都在跑，我闲着。`
                : '全停着呢，今天不干活？';
    return [status, ...FLAVOR_QUIPS.slice(FLAVOR_START), ...FLAVOR_QUIPS.slice(0, FLAVOR_START)];
}
