import { describe, expect, it } from 'vitest';
import {
    appLinkPairEnabled,
    appLinkPairNote,
    appLinkTransportDescription,
    classifyAppLink,
    isDockerBot,
} from './appLinkTopology';

describe('classifyAppLink', () => {
    it('allows same host for local and remote', () => {
        expect(classifyAppLink('local', 'local')).toBe('same_host');
        expect(classifyAppLink('remote:vps', 'remote:vps')).toBe('same_host');
    });

    it('allows local bot to remote app', () => {
        expect(classifyAppLink('local', 'remote:vps')).toBe('local_bot_remote_app');
    });

    it('allows remote bot to local app', () => {
        expect(classifyAppLink('remote:vps', 'local')).toBe('remote_bot_local_app');
    });

    it('allows two remotes as resident link', () => {
        expect(classifyAppLink('remote:a', 'remote:b')).toBe('remote_bot_remote_app');
    });
});

describe('appLinkPairEnabled', () => {
    it('keeps options open until a host is known', () => {
        expect(appLinkPairEnabled(null, 'remote:vps')).toBe(true);
    });

    it('enables P0/P1 desktop tunnels and P2 resident', () => {
        expect(appLinkPairEnabled('local', 'remote:vps')).toBe(true);
        expect(appLinkPairEnabled('remote:vps', 'local')).toBe(true);
        expect(appLinkPairEnabled('remote:a', 'remote:b')).toBe(true);
        expect(appLinkPairNote('local', 'remote:vps')).toBe('（经 SSH 隧道）');
        expect(appLinkPairNote('remote:vps', 'local')).toBe('（经 SSH 隧道）');
        expect(appLinkPairNote('remote:a', 'remote:b')).toBe('（主机常驻隧道）');
    });

    it('same host needs no note', () => {
        expect(appLinkPairEnabled('local', 'local')).toBe(true);
        expect(appLinkPairEnabled('remote:vps', 'remote:vps')).toBe(true);
        expect(appLinkPairNote('local', 'local')).toBe('');
        expect(appLinkPairNote('remote:vps', 'remote:vps')).toBe('');
    });
});

describe('isDockerBot', () => {
    it('recognizes Docker without limiting the supported host pairs', () => {
        expect(isDockerBot('docker')).toBe(true);
        expect(isDockerBot('native')).toBe(false);
        expect(isDockerBot(undefined)).toBe(false);
    });
});

describe('Docker and cross-host connection descriptions', () => {
    it('keeps Docker Bot pairs selectable and labels their bridge and SSH transport', () => {
        expect(appLinkPairEnabled('remote:a', 'remote:a')).toBe(true);
        expect(appLinkPairEnabled('remote:a', 'remote:b')).toBe(true);
        expect(appLinkPairEnabled('remote:a', 'local')).toBe(true);
        expect(appLinkPairNote('remote:a', 'remote:a', 'docker')).toBe('（Docker 桥接）');
        expect(appLinkPairNote('remote:a', 'remote:b', 'docker')).toBe(
            '（Docker 桥接 · 常驻 SSH 隧道）',
        );
        expect(appLinkPairNote('remote:a', 'local', 'docker')).toBe('（Docker 桥接 · SSH 隧道）');
    });

    it('distinguishes forward published ports from the restricted reverse bridge', () => {
        const forward = appLinkTransportDescription('same_host', 'forward_ws', true);
        expect(forward.route).toContain('宿主机发布端口');
        expect(forward.route).toContain('端口映射自动配置');
        const reverse = appLinkTransportDescription('same_host', 'reverse_ws', true);
        expect(reverse.route).toContain('仅绑定 Docker 网桥地址');
        expect(reverse.availability).toContain('桌面端退出后仍保留');
        expect(reverse.availability).toContain('主机重启后');
    });

    it.each(['forward_ws', 'reverse_ws'] as const)(
        'requires Desktop for local/remote %s links',
        (mode) => {
            for (const topology of ['local_bot_remote_app', 'remote_bot_local_app'] as const) {
                for (const docker of [false, true]) {
                    const description = appLinkTransportDescription(topology, mode, docker);
                    expect(description.route).toContain('SSH 隧道');
                    expect(description.availability).toContain('需保持桌面端在线');
                    expect(description.availability).toContain('退出后隧道断开');
                }
            }
        },
    );

    it.each(['forward_ws', 'reverse_ws'] as const)(
        'keeps remote-to-remote %s tunnels resident',
        (mode) => {
            for (const docker of [false, true]) {
                const description = appLinkTransportDescription(
                    'remote_bot_remote_app',
                    mode,
                    docker,
                );
                expect(description.availability).toContain('隧道常驻远端主机');
                expect(description.availability).toContain('桌面端退出后仍保留');
                expect(description.availability).toContain('需重新连接桌面端恢复隧道');
            }
        },
    );
});
