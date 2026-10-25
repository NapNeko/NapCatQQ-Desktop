import { describe, expect, it } from 'vitest';
import { defaultNetworkSettings, networkSettingsEqual, validateNetworkSettings } from './network';

describe('network form validation', () => {
    it.each([
        'http://127.0.0.1:7890',
        'https://proxy.example.com',
        'socks5://[::1]:1080',
        'socks5h://proxy.example.com:1080',
    ])('accepts supported proxy %s', (proxyUrl) => {
        expect(
            validateNetworkSettings({ ...defaultNetworkSettings(), proxyMode: 'manual', proxyUrl }),
        ).toEqual({});
    });

    it.each([
        '',
        'localhost:7890',
        'ftp://proxy.example.com',
        'http://user:secret@proxy.example.com',
        'http://proxy.example.com/path',
        'http://proxy.example.com?token=secret',
        'socks5://proxy.example.com',
        'http://localhost:0',
    ])('rejects invalid manual proxy %s', (proxyUrl) => {
        expect(
            validateNetworkSettings({ ...defaultNetworkSettings(), proxyMode: 'manual', proxyUrl }),
        ).toHaveProperty('proxyUrl');
    });

    it('allows unused empty addresses but rejects unsafe inactive URLs', () => {
        const settings = { ...defaultNetworkSettings(), githubMirror: '', nodeMirror: '' };
        expect(validateNetworkSettings(settings)).toEqual({});
        expect(
            validateNetworkSettings({
                ...settings,
                proxyUrl: 'http://user:secret@proxy.example.com',
            }),
        ).toHaveProperty('proxyUrl');
        expect(
            validateNetworkSettings({
                ...settings,
                nodeMirror: 'https://mirror.example.com/#token',
            }),
        ).toHaveProperty('nodeMirror');
        expect(
            validateNetworkSettings({ ...settings, githubDownloadMode: 'mirror' }),
        ).toHaveProperty('githubMirror');
    });

    it('accepts mirror base paths while rejecting credentials and query strings', () => {
        const settings = { ...defaultNetworkSettings(), nodeDownloadMode: 'mirror' as const };
        expect(
            validateNetworkSettings({
                ...settings,
                nodeMirror: 'https://mirror.example.com/node/dist',
            }),
        ).toEqual({});
        expect(
            validateNetworkSettings({
                ...settings,
                nodeMirror: 'https://mirror.example.com/node?token=secret',
            }),
        ).toHaveProperty('nodeMirror');
        expect(
            validateNetworkSettings({
                ...settings,
                nodeMirror: 'https://user:secret@mirror.example.com/node',
            }),
        ).toHaveProperty('nodeMirror');
    });

    it('keeps uv defaults until a Python index is selected and compares that selection', () => {
        const settings = defaultNetworkSettings();
        expect(settings.pypiIndex).toBe('');
        expect(
            networkSettingsEqual(settings, { ...settings, pypiIndex: 'https://pypi.org/simple' }),
        ).toBe(false);
    });

    it.each(['', 'https://pypi.org/simple', 'https://pypi.tuna.tsinghua.edu.cn/simple/'])(
        'accepts Python package source %s',
        (pypiIndex) => {
            expect(validateNetworkSettings({ ...defaultNetworkSettings(), pypiIndex })).toEqual({});
        },
    );

    it.each([
        'http://pypi.example/simple',
        'file:///pypi/simple',
        'https://user:secret@pypi.example/simple',
        'https://@pypi.example/simple',
        'https://pypi.example/simple?token=secret',
        'https://pypi.example/simple#secret',
    ])('rejects unsafe Python package source %s', (pypiIndex) => {
        expect(validateNetworkSettings({ ...defaultNetworkSettings(), pypiIndex })).toHaveProperty(
            'pypiIndex',
        );
    });

    it('validates separate credentials and byte limits', () => {
        const settings = {
            ...defaultNetworkSettings(),
            proxyMode: 'manual' as const,
            proxyUrl: 'socks5://localhost:1080',
        };
        expect(validateNetworkSettings(settings, 'password')).toHaveProperty('proxyPassword');
        expect(validateNetworkSettings({ ...settings, proxyUsername: 'a:b' })).toHaveProperty(
            'proxyUsername',
        );
        expect(
            validateNetworkSettings({ ...settings, proxyUsername: 'user' }, '密'.repeat(86)),
        ).toHaveProperty('proxyPassword');
    });

    it('accepts bypass domains, IPs and CIDR but rejects URLs, ports and wildcard domains', () => {
        expect(
            validateNetworkSettings({
                ...defaultNetworkSettings(),
                proxyBypass: 'localhost,.example.com,10.0.0.0/8,::1,2001:db8::/32,*',
            }),
        ).toEqual({});
        for (const proxyBypass of [
            'https://example.com',
            'host:8080',
            '*.example.com',
            '10.0.0.1/33',
            '::1/129',
            'a b',
        ]) {
            expect(
                validateNetworkSettings({ ...defaultNetworkSettings(), proxyBypass }),
            ).toHaveProperty('proxyBypass');
        }
    });
});
