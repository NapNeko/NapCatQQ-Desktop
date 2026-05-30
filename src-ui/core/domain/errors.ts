// Tauri invoke 的错误正规化。
//
// 后端 command 返回 Result<T, String>，invoke reject 出来的是个裸字符串，
// 不是 Error 对象。前端到处写 (err as Error).message 会取到 undefined，把
// 后端辛苦拼出来的人话原因（"自动连接被拒绝…"）吞成空白。统一走这里：
// 字符串原样返回，Error 取 message，其余兜底 String()。

export function errorText(err: unknown, fallback = '未知错误'): string {
    if (err == null) return fallback;
    if (typeof err === 'string') return err.trim() || fallback;
    if (err instanceof Error) return err.message.trim() || fallback;
    // Tauri 偶尔抛出 { message } 形状的对象；其余结构化错误兜底 JSON。
    if (typeof err === 'object' && 'message' in err) {
        const m = (err as { message?: unknown }).message;
        if (typeof m === 'string' && m.trim()) return m.trim();
    }
    const s = String(err).trim();
    return s && s !== '[object Object]' ? s : fallback;
}
