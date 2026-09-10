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

const TITLES: Record<DayPhase, string> = {
    morning: '早上好',
    noon: '中午好',
    afternoon: '下午好',
    evening: '晚上好',
    night: '夜深了',
};

// 副标题是吉祥物的口气，不是状态播报（实例为零 / 全停由 fleetHint 顶掉）。
const HINTS: Record<DayPhase, readonly string[]> = {
    morning: [
        '天亮了，昨晚一切照旧。',
        '刚睡醒，先看一眼实例。',
        '早班接上了，你慢慢来。',
        '窗户开了条缝，风挺凉。',
        '早饭吃了吗，我吃了。',
    ],
    noon: [
        '饭点了，机器归我看。',
        '中午这会儿最安静。',
        '日头正高，风扇有点响。',
        '你去吃饭，我不饿。',
        '午休一会儿也行，出事我叫你。',
    ],
    afternoon: [
        '下午容易困，我先撑着。',
        '太阳偏西了。',
        '这会儿最容易走神，别问我怎么知道。',
        '晒着挺舒服，别叫我。',
        '再撑两小时。',
    ],
    evening: [
        '天黑了，灯打开吧。',
        '晚上人多，消息也多。',
        '这个点最热闹，我精神。',
        '窗外开始亮灯了。',
        '你吃过了吗，我又饿了。',
    ],
    night: [
        '这个点还没睡？',
        '夜里很静，我醒着。',
        '早点睡，明天再看。',
        '就我们俩还醒着。',
        '风扇声比白天清楚。',
    ],
};

/// 同一天同一时段固定一句：Hello 卡每分钟重渲一次，副标题不能跟着跳。
export function greetingSeed(now: Date): number {
    return now.getFullYear() * 384 + now.getMonth() * 32 + now.getDate();
}

export function getGreeting(phase: DayPhase, seed = 0): Greeting {
    const hints = HINTS[phase];
    return { title: TITLES[phase], hint: hints[Math.abs(seed) % hints.length] };
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
