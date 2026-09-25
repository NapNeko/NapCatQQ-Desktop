import { describe, expect, it } from 'vitest';
import {
    maibotChatDropsEverything,
    maibotChatScope,
    maibotDefaultChat,
    maibotDefaultConfig,
    validateMaiBotConfig,
} from './maibotConfig';

describe('maibotChatDropsEverything', () => {
    it('flags the upstream default of empty whitelists', () => {
        expect(maibotChatDropsEverything(maibotDefaultChat())).toBe(true);
    });

    it('clears once a group is allowed, the filter is off, or a side is a blacklist', () => {
        expect(maibotChatDropsEverything({ ...maibotDefaultChat(), group_list: ['123'] })).toBe(false);
        expect(maibotChatDropsEverything({ ...maibotDefaultChat(), enable_chat_list_filter: false })).toBe(false);
        expect(maibotChatDropsEverything({ ...maibotDefaultChat(), private_list_type: 'blacklist' })).toBe(false);
    });
});

describe('maibotChatScope', () => {
    it('describes both sides in plain words', () => {
        expect(maibotChatScope(maibotDefaultChat())).toBe('不回群，不回私聊');
        expect(maibotChatScope({ ...maibotDefaultChat(), group_list: ['1', '2'], private_list_type: 'blacklist' })).toBe(
            '2 个群，所有私聊',
        );
        expect(maibotChatScope({ ...maibotDefaultChat(), enable_chat_list_filter: false })).toBe('所有群聊和私聊');
    });
});

describe('validateMaiBotConfig', () => {
    it('accepts the default config', () => {
        expect(validateMaiBotConfig(maibotDefaultConfig(23001))).toEqual([]);
    });

    it('rejects equal ports and non-numeric ids with backend paths', () => {
        const cfg = maibotDefaultConfig(23001);
        cfg.legacy_ws_port = 23001;
        cfg.adapter!.chat.group_list = ['12a'];
        expect(validateMaiBotConfig(cfg).map((i) => i.path)).toEqual([
            'legacy_ws_port',
            'adapter/chat/group_list',
        ]);
    });
});
