// 会话列表按本地日历日区分日期，跨月、跨年与夏令时切换仍保持正确。
export function conversationDate(timestamp: number, now = new Date()) {
    const date = new Date(timestamp);
    if (!timestamp || !Number.isFinite(date.getTime())) return null;
    const day = (value: Date) => `${value.getFullYear()}/${value.getMonth() + 1}/${value.getDate()}`;
    const time = `${String(date.getHours()).padStart(2, '0')}:${String(date.getMinutes()).padStart(2, '0')}`;
    const yesterday = new Date(now.getFullYear(), now.getMonth(), now.getDate() - 1);
    const label = day(date) === day(now) ? time : day(date) === day(yesterday) ? '昨天' : date.getFullYear() === now.getFullYear() ? `${date.getMonth() + 1}/${date.getDate()}` : day(date);
    return { label, title: `${day(date)} ${time}:${String(date.getSeconds()).padStart(2, '0')}`, dateTime: date.toISOString() };
}
