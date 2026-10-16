// 网络设置默认值与表单校验。

import type { NetworkSettings } from '../../ipc/generated/domain/NetworkSettings';

export type { NetworkSettings } from '../../ipc/generated/domain/NetworkSettings';
export type { DownloadMirror } from '../../ipc/generated/domain/DownloadMirror';
export type { NetworkProbeResult } from '../../ipc/generated/domain/NetworkProbeResult';

export function defaultNetworkSettings(): NetworkSettings {
    return {
        proxyMode: 'system',
        proxyUrl: '',
        proxyUsername: '',
        proxyBypass: 'localhost,127.0.0.1,::1',
        githubDownloadMode: 'auto',
        githubMirror: 'https://gh-proxy.com',
        nodeDownloadMode: 'auto',
        nodeMirror: 'https://registry.npmmirror.com/-/binary/node',
    };
}

export function networkSettingsEqual(a: NetworkSettings, b: NetworkSettings): boolean {
    return (Object.keys(defaultNetworkSettings()) as Array<keyof NetworkSettings>).every(
        (key) => a[key] === b[key],
    );
}

function validateUrl(value: string, proxy: boolean): string | undefined {
    if (!value.trim()) return proxy ? '请输入代理地址' : '请输入镜像地址';
    if (byteLength(value.trim()) > 2048) return '地址不能超过 2048 字节';
    let url: URL;
    try {
        url = new URL(value.trim());
    } catch {
        return '请输入包含协议和主机的完整 URL';
    }
    const allowed = proxy ? ['http:', 'https:', 'socks5:', 'socks5h:'] : ['http:', 'https:'];
    if (!allowed.includes(url.protocol)) {
        return proxy ? '支持 HTTP、HTTPS、SOCKS5 和 SOCKS5H' : '镜像地址仅支持 HTTP 或 HTTPS';
    }
    if (!url.hostname || /\s/.test(value.trim())) return '请输入有效的主机地址';
    if (url.username || url.password || /^[a-z][a-z\d+.-]*:\/\/[^/?#]*@/i.test(value.trim())) {
        return '地址中不能包含用户名或密码';
    }
    if (value.includes('?') || value.includes('#')) return '地址中不能包含查询参数或片段';
    if (proxy && url.pathname && url.pathname !== '/') return '代理地址不能包含路径';
    if (proxy && url.protocol.startsWith('socks5') && !url.port) return 'SOCKS 代理需要指定端口';
    if (proxy && url.port === '0') return '代理端口必须在 1 到 65535 之间';
    return undefined;
}

function byteLength(value: string): number {
    return new TextEncoder().encode(value).length;
}

function ipVersion(value: string): number | null {
    const parts = value.split('.');
    if (
        parts.length === 4 &&
        parts.every((part) => /^(0|[1-9]\d{0,2})$/.test(part) && Number(part) <= 255)
    )
        return 4;
    if (value.includes(':') && /^[a-f\d:.]+$/i.test(value)) {
        try {
            new URL(`http://[${value}]/`);
            return 6;
        } catch {
            return null;
        }
    }
    return null;
}

function validateBypass(value: string): string | undefined {
    if (byteLength(value) > 4096) return '代理排除列表不能超过 4096 字节';
    for (const entry of value
        .split(',')
        .map((part) => part.trim())
        .filter(Boolean)) {
        if (entry === '*' || ipVersion(entry)) continue;
        if (entry.includes('/')) {
            const parts = entry.split('/');
            const [address, prefix] = parts;
            const version = ipVersion(address);
            if (
                parts.length === 2 &&
                version &&
                /^\d+$/.test(prefix ?? '') &&
                Number(prefix) <= (version === 4 ? 32 : 128)
            )
                continue;
            return '代理排除列表的 CIDR 地址无效';
        }
        if (/[\s:/@?#*\\]/.test(entry) || /\p{Cc}/u.test(entry)) {
            return '请用逗号分隔域名、IP 或 CIDR，不要填写 URL、端口或域名通配符';
        }
        try {
            const domain = entry.replace(/^\.+/, '');
            if (!domain) return '代理排除列表包含无效的域名';
            new URL(`http://${domain}/`);
        } catch {
            return '代理排除列表包含无效的域名';
        }
    }
    return undefined;
}

export function validateNetworkSettings(settings: NetworkSettings, proxyPassword = '') {
    const errors: Partial<Record<keyof NetworkSettings | 'proxyPassword', string>> = {};
    if (settings.proxyMode === 'manual' || settings.proxyUrl.trim()) {
        const error = validateUrl(settings.proxyUrl, true);
        if (error) errors.proxyUrl = error;
    }
    if (
        byteLength(settings.proxyUsername) > 255 ||
        settings.proxyUsername.includes(':') ||
        /\p{Cc}/u.test(settings.proxyUsername)
    ) {
        errors.proxyUsername = '用户名不能包含控制字符或冒号，且不能超过 255 字节';
    }
    if (settings.proxyMode === 'manual') {
        if (!settings.proxyUsername && proxyPassword)
            errors.proxyPassword = '使用代理密码时请同时填写用户名';
        if (settings.proxyUrl.trim().startsWith('socks') && byteLength(proxyPassword) > 255) {
            errors.proxyPassword = 'SOCKS 代理密码不能超过 255 字节';
        }
    }
    const bypassError = validateBypass(settings.proxyBypass);
    if (bypassError) errors.proxyBypass = bypassError;
    if (settings.githubDownloadMode === 'mirror' || settings.githubMirror.trim()) {
        const error = validateUrl(settings.githubMirror, false);
        if (error) errors.githubMirror = error;
    }
    if (settings.nodeDownloadMode === 'mirror' || settings.nodeMirror.trim()) {
        const error = validateUrl(settings.nodeMirror, false);
        if (error) errors.nodeMirror = error;
    }
    return errors;
}
