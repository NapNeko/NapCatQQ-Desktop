//! Pydantic 时长输入的公共子集与兼容扩展，统一写为微秒精度 ISO 8601。
//! 数字 dotenv 字符串在 Pydantic v2 中不是秒数，写盘前必须规范化。

use std::sync::OnceLock;

use fancy_regex::Regex;

const MICROS: u128 = 1_000_000;
const MAX_MICROS: u128 = 999_999_999 * 86_400 * MICROS + 86_400 * MICROS - 1;

pub(super) fn normalize_duration(raw: &str) -> Option<String> {
    if raw.contains('\n') || raw.contains('\r') {
        return None;
    }
    let raw = raw.trim();
    let negative = raw.starts_with('-');
    let value = raw.strip_prefix('+').or_else(|| raw.strip_prefix('-')).unwrap_or(raw);
    let total = decimal_micros(value, 1, true)
        .or_else(|| clock_micros(value))
        .or_else(|| iso_micros(value))?;
    if total > MAX_MICROS {
        return None;
    }
    let days = total / (86_400 * MICROS);
    let second = total / MICROS % 86_400;
    let microsecond = total % MICROS;
    let sign = if negative && total != 0 { "-" } else { "" };
    let day_part = if days == 0 { String::new() } else { format!("{days}D") };
    let fraction = if microsecond == 0 {
        String::new()
    } else {
        format!(".{:06}", microsecond).trim_end_matches('0').to_string()
    };
    Some(format!("{sign}P{day_part}T{second}{fraction}S"))
}

fn decimal_micros(value: &str, seconds: u128, round: bool) -> Option<u128> {
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    if whole.is_empty() || !whole.bytes().all(|b| b.is_ascii_digit())
        || !fraction.bytes().all(|b| b.is_ascii_digit())
        || (value.contains('.') && fraction.is_empty())
    {
        return None;
    }
    let total = whole.parse::<u128>().ok()?.checked_mul(seconds)?.checked_mul(MICROS)?;
    let fraction = &fraction[..fraction.len().min(18)];
    let fractional = if fraction.is_empty() {
        0
    } else {
        let numerator = fraction.parse::<u128>().ok()?.checked_mul(seconds)?.checked_mul(MICROS)?;
        let denominator = 10_u128.pow(fraction.len() as u32);
        numerator.checked_add(if round { denominator / 2 } else { 0 })? / denominator
    };
    total.checked_add(fractional)
}

fn clock_micros(value: &str) -> Option<u128> {
    static CLOCK: OnceLock<Regex> = OnceLock::new();
    let pattern = CLOCK.get_or_init(|| Regex::new(
        r"(?i)^(?:([0-9]+)(?: ?(?:days?|d),? ?| ))?(?:([0-9]+):([0-9]{2}):([0-9]{2}(?:\.[0-9]+)?))?$"
    ).expect("constant duration regex"));
    let captures = pattern.captures(value).ok()??;
    let days = captures.get(1).map(|v| v.as_str()).unwrap_or("0");
    let hours = captures.get(2).map(|v| v.as_str()).unwrap_or("0");
    let minutes = captures.get(3).map(|v| v.as_str()).unwrap_or("0");
    let seconds = captures.get(4).map(|v| v.as_str()).unwrap_or("0");
    if captures.get(1).is_none() && captures.get(2).is_none()
        || minutes.parse::<u32>().ok()? >= 60
        || seconds.split('.').next()?.parse::<u32>().ok()? >= 60
    {
        return None;
    }
    decimal_micros(days, 86_400, false)?
        .checked_add(decimal_micros(hours, 3600, false)?)?
        .checked_add(decimal_micros(minutes, 60, false)?)?
        .checked_add(decimal_micros(seconds, 1, false)?)
}

fn iso_micros(value: &str) -> Option<u128> {
    let value = value.strip_prefix('P')?;
    let bytes = value.as_bytes();
    let mut position = 0;
    let mut in_time = false;
    let mut has_time = false;
    let mut last_rank = None;
    let mut fractional = false;
    let mut total = 0_u128;
    while position < bytes.len() {
        if bytes[position] == b'T' {
            if in_time || fractional {
                return None;
            }
            in_time = true;
            position += 1;
            continue;
        }
        if fractional {
            return None;
        }
        let start = position;
        while position < bytes.len() && (bytes[position].is_ascii_digit() || bytes[position] == b'.') {
            position += 1;
        }
        let number = &value[start..position];
        let (rank, seconds) = match (in_time, *bytes.get(position)?) {
            (false, b'Y') => (0, 365 * 86_400),
            (false, b'M') => (1, 30 * 86_400),
            (false, b'W') => (2, 7 * 86_400),
            (false, b'D') => (3, 86_400),
            (true, b'H') => (4, 3600),
            (true, b'M') => (5, 60),
            (true, b'S') => (6, 1),
            _ => return None,
        };
        if last_rank.is_some_and(|last| rank <= last) {
            return None;
        }
        total = total.checked_add(decimal_micros(number, seconds, true)?)?;
        fractional = number.contains('.');
        has_time |= in_time;
        last_rank = Some(rank);
        position += 1;
    }
    (last_rank.is_some() && (!in_time || has_time)).then_some(total)
}

#[cfg(test)]
mod tests {
    use super::normalize_duration;

    #[test]
    fn accepts_seconds_clock_and_iso_with_signs_and_microseconds() {
        for (raw, expected) in [
            ("120", "PT120S"),
            ("-0.000001", "-PT0.000001S"),
            ("+1.25", "PT1.25S"),
            ("0.0000009", "PT0.000001S"),
            ("PT0.0000009S", "PT0.000001S"),
            ("00:00:00.0000009", "PT0S"),
            ("00:02:00", "PT120S"),
            ("-1 day, 02:03:04.000005", "-P1DT7384.000005S"),
            ("1 02:03:04", "P1DT7384S"),
            ("P1DT2H", "P1DT7200S"),
            ("+PT0.000001S", "PT0.000001S"),
            ("-PT1.25S", "-PT1.25S"),
            ("P1W", "P7DT0S"),
            ("P1.5D", "P1DT43200S"),
        ] {
            assert_eq!(normalize_duration(raw).as_deref(), Some(expected), "{raw}");
        }
    }

    #[test]
    fn rejects_invalid_incomplete_repeated_and_out_of_range_durations() {
        for raw in ["", "abc", "NaN", "Infinity", "P", "PT", "P1DT", "P-1D", "P1D2D",
            "PT1S2M", "PT1.5H30M", "00:60:00", "00:00:99", "120\n", "P1000000000D"] {
            assert_eq!(normalize_duration(raw), None, "{raw}");
        }
    }
}
