// NeoBot 面板数据的收窄与解析。
//
// 面板的响应是**扁平信封**（`{"ok": true, ...键平铺...}`，见 dashboard/api.py 的 _json_ok），
// 而且没有对外的 schema 承诺。所以这里一律**防御式解析**：缺键给默认值、类型不符就丢，
// 绝不 of 直接断言——面板升一版就让整页白屏是最糟的体验。
//
// 键名来自后端 handler 的 _json_ok（/api/overview），不是猜的。

/** /api/overview 的一条配置提示（面板首页用来提醒「有缺口但没到报错」） */
export interface NeoBotOverviewNotice {
    level: string;
    text: string;
    hint?: string;
}

export interface NeoBotOverview {
    online: boolean;
    appName: string;
    appVersion: string;
    botNickname: string;
    botUserId: string | null;
    avatarUrl: string;
    uptimeSeconds: number;
    todayMessages: number;
    totalMessages: number;
    pluginsLoaded: number;
    pluginsTotal: number;
    pluginsError: number;
    latencyMs: number | null;
    pythonVersion: string | null;
    hostname: string | null;
    standby: boolean;
    notices: NeoBotOverviewNotice[];
}

export function asRecord(raw: unknown): Record<string, unknown> | null {
    return typeof raw === 'object' && raw !== null && !Array.isArray(raw)
        ? (raw as Record<string, unknown>)
        : null;
}

export function asString(v: unknown, fallback = ''): string {
    return typeof v === 'string' ? v : fallback;
}

export function asNumber(v: unknown, fallback = 0): number {
    return typeof v === 'number' && Number.isFinite(v) ? v : fallback;
}

function asNullableNumber(v: unknown): number | null {
    return typeof v === 'number' && Number.isFinite(v) ? v : null;
}

export function asBool(v: unknown): boolean {
    return v === true;
}

/** 机器人 QQ 号：面板可能给数字也可能给字符串，统一成字符串（超长数字别被 JS 精度吃掉） */
function asIdString(v: unknown): string | null {
    if (typeof v === 'string' && v.trim()) return v.trim();
    if (typeof v === 'number' && Number.isFinite(v)) return String(v);
    return null;
}

function parseNotices(v: unknown): NeoBotOverviewNotice[] {
    if (!Array.isArray(v)) return [];
    return v
        .map((item) => asRecord(item))
        .filter((item): item is Record<string, unknown> => item !== null)
        .map((item) => ({
            level: asString(item.level, 'info'),
            text: asString(item.text),
            hint: typeof item.hint === 'string' ? item.hint : undefined,
        }))
        .filter((n) => n.text !== '');
}

/**
 * 把面板回包收窄成 NeoBotOverview。
 *
 * 传进来的应当是 AppPanelResult.data。不是对象（或为空）返回 null——调用方据此显示
 * 「面板没给数据」，而不是渲染一屏 0。
 */
export function parseNeoBotOverview(raw: unknown): NeoBotOverview | null {
    const r = asRecord(raw);
    if (!r) return null;
    return {
        online: asBool(r.online),
        appName: asString(r.app_name),
        appVersion: asString(r.app_version),
        botNickname: asString(r.bot_nickname),
        botUserId: asIdString(r.bot_user_id),
        avatarUrl: asString(r.avatar_url),
        uptimeSeconds: asNumber(r.uptime_seconds),
        todayMessages: asNumber(r.today_messages),
        totalMessages: asNumber(r.total_messages),
        pluginsLoaded: asNumber(r.plugins_loaded),
        pluginsTotal: asNumber(r.plugins_total),
        pluginsError: asNumber(r.plugins_error),
        latencyMs: asNullableNumber(r.latency_ms),
        pythonVersion: typeof r.python_version === 'string' ? r.python_version : null,
        hostname: typeof r.hostname === 'string' ? r.hostname : null,
        standby: asBool(r.standby),
        notices: parseNotices(r.notices),
    };
}

/** 把秒数说成人话：面板首页显示运行时长用 */
export function formatUptime(seconds: number): string {
    if (!Number.isFinite(seconds) || seconds <= 0) return '—';
    const s = Math.floor(seconds);
    const d = Math.floor(s / 86400);
    const h = Math.floor((s % 86400) / 3600);
    const m = Math.floor((s % 3600) / 60);
    if (d > 0) return `${d} 天 ${h} 小时`;
    if (h > 0) return `${h} 小时 ${m} 分`;
    if (m > 0) return `${m} 分`;
    return `${s} 秒`;
}