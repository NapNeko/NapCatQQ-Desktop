import { beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({ native: true, invoke: vi.fn(), save: vi.fn(), listen: vi.fn() }));
vi.mock('../ipc/transport', () => ({
    get isTauri() {
        return ipc.native;
    },
    invoke: ipc.invoke,
    listen: ipc.listen,
    saveZipFile: ipc.save,
    pickDirectory: vi.fn(),
    pickZipFile: vi.fn(),
}));

beforeEach(() => {
    localStorage.clear();
    ipc.native = true;
    ipc.invoke.mockReset();
    ipc.save.mockReset();
    ipc.listen.mockReset();
});

describe('complete configuration transfer', () => {
    it('delivers imported file labels from the unwrapped IPC notification', async () => {
        const { configTransferService } = await import('./config-transfer.service');
        const received: string[][] = [];
        ipc.listen.mockImplementation(async (_event, callback) => {
            callback(['应用设置', 'API 调试工作区']);
            return () => {};
        });
        await configTransferService.onImported((files) => received.push(files));
        expect(received).toEqual([['应用设置', 'API 调试工作区']]);
    });
    it('includes saved UI preferences without terminal history or unrelated storage', async () => {
        const { configTransferService } = await import('./config-transfer.service');
        localStorage.setItem(
            'ncd.terminal.prefs.v1',
            '{"fontSize":17,"snippets":[{"label":"状态","command":"pwd"}]}',
        );
        localStorage.setItem('ncd.chat.ui.v1', '{"listWidth":300}');
        localStorage.setItem('ncd:bot_custom_order:v1', '["10002","10001"]');
        localStorage.setItem('ncd.maibot.chat.name.instance-1', '桌面用户');
        localStorage.setItem('ncd.terminal.recents.v1', '{"local":["private command"]}');
        localStorage.setItem('ncd_perf_marks', '1');
        ipc.save.mockResolvedValue('D:/backup/config.ZIP');
        ipc.invoke.mockResolvedValue({
            export_path: 'D:/backup/config.ZIP',
            files: ['界面与终端偏好'],
        });

        await configTransferService.export();

        expect(ipc.invoke).toHaveBeenCalledWith('export_config', {
            destPath: 'D:/backup/config.ZIP',
            frontendPreferences: {
                version: 1,
                storage: {
                    'ncd.terminal.prefs.v1':
                        '{"fontSize":17,"snippets":[{"label":"状态","command":"pwd"}]}',
                    'ncd.chat.ui.v1': '{"listWidth":300}',
                    'ncd:bot_custom_order:v1': '["10002","10001"]',
                    'ncd.maibot.chat.name.instance-1': '桌面用户',
                },
            },
        });
    });

    it('keeps cancellation successful even when stored preferences cannot be exported', async () => {
        const { configTransferService } = await import('./config-transfer.service');
        localStorage.setItem('ncd.terminal.prefs.v1', 'invalid JSON');
        ipc.save.mockResolvedValue(null);
        await expect(configTransferService.export()).resolves.toBeNull();
        expect(ipc.invoke).not.toHaveBeenCalled();
    });

    it('reports unsupported browser export instead of returning a fake successful file', async () => {
        const { configTransferService } = await import('./config-transfer.service');
        ipc.native = false;
        await expect(configTransferService.export()).rejects.toThrow('桌面');
    });

    it('restores preferences into both storage and existing preference stores', async () => {
        const { configTransferService } = await import('./config-transfer.service');
        const { terminalPrefs } = await import('../../hooks/terminal/terminalPrefs');
        const { botSortStore } = await import('../../hooks/bot/useBotSort');
        terminalPrefs.patch({ fontSize: 13 });
        botSortStore.set(['old-bot']);
        localStorage.setItem('unrelated-key', 'keep');
        ipc.invoke.mockResolvedValue({
            files: ['界面与终端偏好'],
            skipped: [],
            frontend_preferences: {
                version: 1,
                storage: {
                    'ncd.terminal.prefs.v1': '{"fontSize":21}',
                    'ncd:bot_custom_order:v1': '["10002","10001"]',
                },
            },
        });

        await configTransferService.import('D:/backup/config.zip');

        expect(terminalPrefs.get().fontSize).toBe(21);
        expect(botSortStore.get()).toEqual(['10002', '10001']);
        expect(localStorage.getItem('unrelated-key')).toBe('keep');
    });

    it('clears allowlisted preferences omitted from the imported snapshot but keeps unrelated storage', async () => {
        const { configTransferService } = await import('./config-transfer.service');
        const { terminalPrefs } = await import('../../hooks/terminal/terminalPrefs');
        terminalPrefs.patch({ fontSize: 13 });
        localStorage.setItem('ncd.terminal.layout.v1', '{"height":300}');
        localStorage.setItem('ncd.maibot.chat.name.instance-1', '旧昵称');
        localStorage.setItem('unrelated-key', 'keep');
        ipc.invoke.mockResolvedValue({
            files: ['界面与终端偏好'],
            skipped: [],
            frontend_preferences: {
                version: 1,
                storage: { 'ncd.terminal.prefs.v1': '{"fontSize":21}' },
            },
        });

        await configTransferService.import('config.zip');

        expect(terminalPrefs.get().fontSize).toBe(21);
        expect(localStorage.getItem('ncd.terminal.layout.v1')).toBeNull();
        expect(localStorage.getItem('ncd.maibot.chat.name.instance-1')).toBeNull();
        expect(localStorage.getItem('unrelated-key')).toBe('keep');
    });

    it('restores removed preferences when a later write fails and rollback runs', async () => {
        const { configTransferService } = await import('./config-transfer.service');
        localStorage.setItem('ncd.terminal.layout.v1', '{"height":300}');
        const set = Storage.prototype.setItem;
        vi.spyOn(Storage.prototype, 'setItem').mockImplementation(function (
            this: Storage,
            key,
            value,
        ) {
            if (key === 'ncd.terminal.prefs.v1')
                throw new DOMException('Storage full', 'QuotaExceededError');
            set.call(this, key, value);
        });
        ipc.invoke.mockResolvedValue({
            files: ['界面与终端偏好'],
            skipped: [],
            frontend_preferences: {
                version: 1,
                storage: { 'ncd.terminal.prefs.v1': '{"fontSize":19}' },
            },
        });

        const result = await configTransferService.import('config.zip');

        expect(result).toMatchObject({
            frontendPreferencesError: expect.stringContaining('Storage full'),
        });
        expect(localStorage.getItem('ncd.terminal.layout.v1')).toBe('{"height":300}');
        expect(localStorage.getItem('ncd.terminal.prefs.v1')).toBeNull();
    });

    it('leaves local preferences untouched when backend validation fails', async () => {
        const { configTransferService } = await import('./config-transfer.service');
        localStorage.setItem('ncd.chat.ui.v1', '{"listWidth":280}');
        ipc.invoke.mockRejectedValue(new Error('invalid config'));
        await expect(configTransferService.import('bad.zip')).rejects.toThrow('invalid config');
        expect(localStorage.getItem('ncd.chat.ui.v1')).toBe('{"listWidth":280}');
    });

    it('rejects unknown preference keys before changing any existing preference', async () => {
        const { configTransferService } = await import('./config-transfer.service');
        localStorage.setItem('ncd.chat.ui.v1', '{"listWidth":280}');
        ipc.invoke.mockResolvedValue({
            files: ['应用设置'],
            skipped: [],
            frontend_preferences: {
                version: 1,
                storage: { 'ncd.chat.ui.v1': '{"listWidth":310}', 'untrusted-key': 'overwrite' },
            },
        });

        const result = await configTransferService.import('config.zip');

        expect(result).toMatchObject({
            files: ['应用设置'],
            frontendPreferencesError: expect.any(String),
        });
        expect(localStorage.getItem('ncd.chat.ui.v1')).toBe('{"listWidth":280}');
        expect(localStorage.getItem('untrusted-key')).toBeNull();
    });

    it('rolls back browser writes and keeps the successful file result when storage fills up', async () => {
        const { configTransferService } = await import('./config-transfer.service');
        localStorage.setItem('ncd.terminal.prefs.v1', '{"fontSize":13}');
        const set = Storage.prototype.setItem;
        vi.spyOn(Storage.prototype, 'setItem').mockImplementation(function (
            this: Storage,
            key,
            value,
        ) {
            if (key === 'ncd.chat.ui.v1')
                throw new DOMException('Storage full', 'QuotaExceededError');
            set.call(this, key, value);
        });
        ipc.invoke.mockResolvedValue({
            files: ['应用设置'],
            skipped: [],
            frontend_preferences: {
                version: 1,
                storage: {
                    'ncd.terminal.prefs.v1': '{"fontSize":19}',
                    'ncd.chat.ui.v1': '{"listWidth":300}',
                },
            },
        });

        const result = await configTransferService.import('config.zip');

        expect(result).toMatchObject({
            files: ['应用设置'],
            frontendPreferencesError: expect.stringContaining('Storage full'),
        });
        expect(localStorage.getItem('ncd.terminal.prefs.v1')).toBe('{"fontSize":13}');
        expect(localStorage.getItem('ncd.chat.ui.v1')).toBeNull();
    });

    it('rejects malformed terminal settings before restoring another preference', async () => {
        const { configTransferService } = await import('./config-transfer.service');
        localStorage.setItem('ncd.chat.ui.v1', '{"listWidth":280}');
        ipc.invoke.mockResolvedValue({
            files: ['应用设置'],
            skipped: [],
            frontend_preferences: {
                version: 1,
                storage: {
                    'ncd.chat.ui.v1': '{"listWidth":300}',
                    'ncd.terminal.prefs.v1': '{"fontSize":"bad"}',
                },
            },
        });

        const result = await configTransferService.import('config.zip');

        expect(result).toMatchObject({ frontendPreferencesError: expect.any(String) });
        expect(localStorage.getItem('ncd.chat.ui.v1')).toBe('{"listWidth":280}');
    });
});
