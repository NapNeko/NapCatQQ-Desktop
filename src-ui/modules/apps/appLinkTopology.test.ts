import { describe, expect, it } from 'vitest';
import {
    appLinkPairEnabled,
    appLinkPairNote,
    classifyAppLink,
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
});
