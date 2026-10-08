// 账号密码类 WebUI 的假账号：新建时按请求种入，重置时换密码。
// 目前只有 AstrBot 走 user_password，但视图逻辑不绑定框架，供清单里以后加同类端复用。
import type { AppInstance, AppWebUiAccount } from '../../types';
import { mockAppFrameworks } from './manifests';

export const mockWebUiAccounts = new Map<string, { username: string; password: string | null }>([
    ['ab12cd34', { username: 'astrbot', password: 'Mock2024astrbot' }],
]);

export function mockAccountView(inst: AppInstance): AppWebUiAccount | null {
    const manifest = mockAppFrameworks.find((m) => m.id === inst.framework_id);
    if (manifest?.webui_auth !== 'user_password') return null;
    const acct = mockWebUiAccounts.get(inst.id) ?? { username: 'astrbot', password: null };
    return {
        username: acct.username,
        password: acct.password ?? undefined,
        password_matches: acct.password ? true : undefined,
        can_reset: inst.state !== 'running',
    };
}

export function mockGeneratePassword(): string {
    const pool = 'ABCDEFGHJKLMNPQRSTUVWXYZabcdefghjkmnpqrstuvwxyz23456789';
    let out = 'Aa1';
    for (let i = 0; i < 21; i += 1) out += pool[Math.floor(Math.random() * pool.length)];
    return out;
}
