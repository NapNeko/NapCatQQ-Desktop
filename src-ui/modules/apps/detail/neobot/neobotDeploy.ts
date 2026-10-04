// 快捷部署（/api/deploy/status）的收窄。
//
// 面板把「还差哪几项」逐条判好（含出厂占位不算配好的判断），并给出反向 WS 的**生效**
// 监听信息——地址与 access token 就是 NapCat 侧要填的两样。键名取自后端 handler，
// 不是猜的。

import { asBool, asNumber, asRecord, asString } from './neobotPanel';

export interface NeoBotDeployStep {
    key: string;
    label: string;
    /** 必填项没做就不能正常工作；选填项只是少个功能 */
    required: boolean;
    done: boolean;
    hint: string;
}

/** 反向 WS 的生效监听信息：NapCat 侧要填的就是 url* 与 token */
export interface NeoBotOneBotConn {
    host: string;
    port: number;
    /** 同机连接用的地址 */
    urlLocal: string;
    /** 局域网连接用的地址（NapCat 在另一台机器时） */
    urlLan: string;
    token: string;
    /** token 是否已启用校验；为空说明不校验握手 */
    tokenEnabled: boolean;
    /** 面板建议的路径（服务端不限制路径，这只是 NapCat 常用的那个） */
    pathHint: string;
    /** 面板给出的提醒（例如监听在 0.0.0.0 但没设 token） */
    warning: string;
}

export interface NeoBotDeployValues {
    /** 机器人 QQ 号；出厂占位是 '0'，未配好时也是它 */
    botAccount: string;
    botNickName: string;
    botData: string;
    adminAccounts: string[];
}

export interface NeoBotDeployStatus {
    steps: NeoBotDeployStep[];
    onebot: NeoBotOneBotConn;
    values: NeoBotDeployValues;
    /** 本体配置版本号，改配置时要带回去 */
    revision: string;
    /** .env 的版本号，改密钥时要带回去 */
    envRevision: string | null;
    /** 必填项是否全做完 */
    ready: boolean;
}

function parseSteps(v: unknown): NeoBotDeployStep[] {
    if (!Array.isArray(v)) return [];
    return v
        .map((x) => asRecord(x))
        .filter((x): x is Record<string, unknown> => x !== null)
        .map((s) => ({
            key: asString(s.key),
            label: asString(s.label) || asString(s.key),
            // 拿不到 required 时按必填算：宁可多提示，也别把必填项漏掉
            required: s.required === undefined ? true : asBool(s.required),
            done: asBool(s.done),
            hint: asString(s.hint),
        }))
        .filter((s) => s.key !== '');
}

function parseOneBot(v: unknown): NeoBotOneBotConn {
    const r = asRecord(v) ?? {};
    return {
        host: asString(r.host),
        port: asNumber(r.port),
        urlLocal: asString(r.url_local),
        urlLan: asString(r.url_lan),
        token: asString(r.token),
        tokenEnabled: asBool(r.token_enabled),
        pathHint: asString(r.path_hint),
        warning: asString(r.warning),
    };
}

function parseValues(v: unknown): NeoBotDeployValues {
    const r = asRecord(v) ?? {};
    const admins = Array.isArray(r.admin_accounts) ? r.admin_accounts : [];
    return {
        botAccount: asString(r.bot_account),
        botNickName: asString(r.bot_nick_name),
        botData: asString(r.bot_data),
        adminAccounts: admins
            .map((x) => (typeof x === 'string' ? x.trim() : ''))
            .filter((x) => x !== ''),
    };
}

export function parseNeoBotDeployStatus(raw: unknown): NeoBotDeployStatus | null {
    const r = asRecord(raw);
    // steps 是这一页的骨架，没有它就没法说「还差什么」——当 malformed 而不是渲染空页
    if (!r || !Array.isArray(r.steps)) return null;
    return {
        steps: parseSteps(r.steps),
        onebot: parseOneBot(r.onebot),
        values: parseValues(r.values),
        revision: asString(r.revision),
        envRevision: typeof r.env_revision === 'string' ? r.env_revision : null,
        ready: asBool(r.ready),
    };
}

/** 必填但还没做的项——页面顶部据此说「还差 N 项」 */
export function missingRequiredSteps(status: NeoBotDeployStatus): NeoBotDeployStep[] {
    return status.steps.filter((s) => s.required && !s.done);
}
