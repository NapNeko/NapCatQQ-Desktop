// 概览 Hello 卡的「此刻天色」：时段划分、问候语、日月位置、星空布局。
// 纯函数，零 React；时段边界只在这里定一次，问候和天空共用。

export type DayPhase = 'morning' | 'noon' | 'afternoon' | 'evening' | 'night';

export function getDayPhase(hour: number): DayPhase {
    if (hour >= 5 && hour < 11) return 'morning';
    if (hour >= 11 && hour < 14) return 'noon';
    if (hour >= 14 && hour < 18) return 'afternoon';
    if (hour >= 18 && hour < 23) return 'evening';
    return 'night';
}

export interface Greeting {
    title: string;
    hint: string;
}

const GREETINGS: Record<DayPhase, Greeting> = {
    morning: { title: '早上好', hint: '美好的一天，NapCat 正在守护你的机器人实例。' },
    noon: { title: '中午好', hint: '午间时刻，NapCat 替你盯着机器人实例。' },
    afternoon: { title: '下午好', hint: '下午时光，NapCat 继续守护你的实例与连接。' },
    evening: { title: '晚上好', hint: '夜晚安宁，NapCatQQ 持续为你守护消息与连接。' },
    night: { title: '夜深了', hint: '夜深人静，后台服务还在替你守着。' },
};

export function getGreeting(phase: DayPhase): Greeting {
    return GREETINGS[phase];
}

export type CelestialKind = 'sun' | 'moon';

export interface CelestialPosition {
    kind: CelestialKind;
    /** 卡片宽度百分比 0–100。 */
    x: number;
    /** 卡片高度百分比 0–100。 */
    y: number;
    /** 0 = 刚升起 / 落下，1 = 中天。 */
    altitude: number;
}

const SUN_RISE = 5;
const SUN_SET = 18;

/// 白天太阳、夜里月亮沿同一条弧线从卡片中部走到右上再落下。
/// 弧线整体压在右半边：左边是文字区，天体不去抢可读性。
export function celestialPosition(hour: number, minute: number): CelestialPosition {
    const h = hour + minute / 60;
    const isDay = h >= SUN_RISE && h < SUN_SET;
    const t = isDay
        ? (h - SUN_RISE) / (SUN_SET - SUN_RISE)
        : (((h - SUN_SET) % 24) + 24) % 24 / (24 - (SUN_SET - SUN_RISE));
    const altitude = Math.sin(Math.PI * t);
    return {
        kind: isDay ? 'sun' : 'moon',
        x: 42 + 52 * t,
        y: 62 - 50 * altitude,
        altitude,
    };
}

export interface Star {
    x: number;
    y: number;
    /** 直径 px。 */
    size: number;
    /** 闪烁周期 s。 */
    period: number;
    /** 闪烁起始相位 s（负值给 animation-delay）。 */
    delay: number;
}

// 固定种子的 LCG：同一时段每次渲染星星都在同一位置，不会重挂就跳一次。
function lcg(seed: number): () => number {
    let s = seed >>> 0;
    return () => {
        s = (s * 1664525 + 1013904223) >>> 0;
        return s / 0x100000000;
    };
}

/// 文字区在左下，星星只落在顶部一条带 + 右半边，不压在问候语上。
export function starField(count: number, seed = 7): Star[] {
    const rand = lcg(seed);
    const stars: Star[] = [];
    for (let i = 0; i < count; i += 1) {
        const x = 6 + rand() * 90;
        const y = x < 55 ? 5 + rand() * 14 : 5 + rand() * 62;
        stars.push({
            x,
            y,
            size: 2 + rand() * 2,
            period: 2.4 + rand() * 2.4,
            delay: -rand() * 4,
        });
    }
    return stars;
}

export function starCountFor(phase: DayPhase): number {
    if (phase === 'night') return 18;
    if (phase === 'evening') return 7;
    return 0;
}

export interface Cloud {
    /** 顶部百分比。 */
    y: number;
    /** 宽 px；高按比例。 */
    width: number;
    /** 飘完一趟的秒数。 */
    duration: number;
    /** 起始相位 s（负值给 animation-delay），让三朵云不排队出场。 */
    delay: number;
    /** 0–1，越远越淡越小。 */
    depth: number;
}

/// 白天三朵云在顶部一条带里横飘；夜里让位给星星。
export function cloudField(phase: DayPhase): Cloud[] {
    if (phase === 'night') return [];
    const clouds: Cloud[] = [
        { y: 8, width: 150, duration: 96, delay: -12, depth: 0.35 },
        { y: 24, width: 110, duration: 128, delay: -70, depth: 0.7 },
        { y: 15, width: 84, duration: 150, delay: -40, depth: 1 },
    ];
    return phase === 'evening' ? clouds.slice(0, 2) : clouds;
}
