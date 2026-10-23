// 与 nonebot2/duration.rs 一致：ISO 保留原文；秒数与无歧义 clock 转成 ISO 微秒时长。
// 负数 day/clock 的版本语义有歧义，保留原字段并拒绝保存，要求使用秒数或 -ISO。
const MICROS = 1_000_000n;
const MAX_MICROS = 999_999_999n * 86_400n * MICROS + 86_400n * MICROS - 1n;

function decimalMicros(value: string, seconds: bigint, round: boolean): bigint | null {
    const match = /^([0-9]+)(?:\.([0-9]+))?$/.exec(value);
    if (!match) return null;
    try {
        const whole = BigInt(match[1]) * seconds * MICROS;
        const fraction = (match[2] ?? '').slice(0, 18);
        if (!fraction) return whole;
        const denominator = 10n ** BigInt(fraction.length);
        const numerator = BigInt(fraction) * seconds * MICROS;
        return whole + (numerator + (round ? denominator / 2n : 0n)) / denominator;
    } catch {
        return null;
    }
}

function clockMicros(value: string): bigint | null {
    const match =
        /^(?:([0-9]+)(?: ?(?:days?|d),? ?| ))?(?:([0-9]+):([0-9]{2}):([0-9]{2}(?:\.[0-9]+)?))?$/i.exec(
            value,
        );
    if (
        !match ||
        (!match[1] && !match[2]) ||
        Number(match[3] ?? 0) >= 60 ||
        Number((match[4] ?? '0').split('.')[0]) >= 60
    )
        return null;
    return (
        decimalMicros(match[1] ?? '0', 86_400n, false)! +
        decimalMicros(match[2] ?? '0', 3600n, false)! +
        decimalMicros(match[3] ?? '0', 60n, false)! +
        decimalMicros(match[4] ?? '0', 1n, false)!
    );
}

function isoMicros(value: string): bigint | null {
    if (!value.startsWith('P')) return null;
    let rest = value.slice(1);
    let inTime = false;
    let hasTime = false;
    let lastRank = -1;
    let fractional = false;
    let total = 0n;
    while (rest) {
        if (rest.startsWith('T')) {
            if (inTime || fractional) return null;
            inTime = true;
            rest = rest.slice(1);
            continue;
        }
        if (fractional) return null;
        const match = /^([0-9]+(?:\.[0-9]+)?)([YMWDHMS])/.exec(rest);
        if (!match) return null;
        const unit = match[2];
        const ranks = inTime ? ['H', 'M', 'S'] : ['Y', 'M', 'W', 'D'];
        const index = ranks.indexOf(unit);
        const rank = index + (inTime ? 4 : 0);
        if (index < 0 || rank <= lastRank) return null;
        const factors = inTime
            ? [3600n, 60n, 1n]
            : [365n * 86_400n, 30n * 86_400n, 7n * 86_400n, 86_400n];
        const amount = decimalMicros(match[1], factors[index], true);
        if (amount == null) return null;
        total += amount;
        fractional = match[1].includes('.');
        hasTime ||= inTime;
        lastRank = rank;
        rest = rest.slice(match[0].length);
    }
    return lastRank >= 0 && (!inTime || hasTime) ? total : null;
}

export function normalizeNoneBot2Duration(raw: string): string | null {
    if (/[\r\n]/.test(raw)) return null;
    const value = raw.trim();
    const negative = value.startsWith('-');
    const unsigned = /^[+-]/.test(value) ? value.slice(1) : value;
    if (unsigned.startsWith('P')) {
        const total = isoMicros(unsigned);
        return total != null && total <= MAX_MICROS ? value : null;
    }
    const total = decimalMicros(unsigned, 1n, false) ?? (negative ? null : clockMicros(unsigned));
    if (total == null || total > MAX_MICROS) return null;
    const days = total / (86_400n * MICROS);
    const second = (total / MICROS) % 86_400n;
    const microsecond = total % MICROS;
    const sign = negative && total !== 0n ? '-' : '';
    const dayPart = days === 0n ? '' : days + 'D';
    const fraction =
        microsecond === 0n ? '' : '.' + String(microsecond).padStart(6, '0').replace(/0+$/, '');
    return sign + 'P' + dayPart + 'T' + second + fraction + 'S';
}
