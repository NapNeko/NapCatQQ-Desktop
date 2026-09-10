// 账号密码类 WebUI 的前端口令校验；规则与后端 `dashboard_auth::validate_password`（AstrBot 上游策略）一致。

export const WEBUI_PASSWORD_MIN_LEN = 8;

export function validateWebUiPassword(raw: string): string | null {
    if (!raw) return null;
    if (raw.length < WEBUI_PASSWORD_MIN_LEN) return `至少 ${WEBUI_PASSWORD_MIN_LEN} 位`;
    if (!/[A-Z]/.test(raw)) return '需含大写字母';
    if (!/[a-z]/.test(raw)) return '需含小写字母';
    if (!/[0-9]/.test(raw)) return '需含数字';
    return null;
}

export function validateWebUiUsername(raw: string): string | null {
    const v = raw.trim();
    if (!v) return null;
    if (/\s/.test(v)) return '不能含空格';
    return null;
}
