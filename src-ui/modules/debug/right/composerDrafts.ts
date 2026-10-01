// 输入框的草稿，按 Bot + 会话记在模块里：收起右栏、切走路由、换会话再回来，没发出去的都还在。
// 只活在这次打开应用期间，不落盘。
// 一份草稿 = 手打文字 + @ 名单 + 构建器段（ComposerEntry），输入框和构建器读写同一份。

import { EMPTY_ENTRY, type ComposerEntry } from '../../../core/domain/debug/messageBuilder';

const drafts = new Map<string, ComposerEntry>();

export function draftKey(botId: string, session: string): string {
    return `${botId}|${session}`;
}

export function readEntry(key: string): ComposerEntry {
    return drafts.get(key) ?? EMPTY_ENTRY;
}

export function writeEntry(key: string, entry: ComposerEntry): void {
    if (entry.text === '' && entry.mentions.length === 0 && entry.rich.length === 0) drafts.delete(key);
    else drafts.set(key, entry);
}

/** 测试用 */
export function _resetComposerDraftsForTests(): void {
    drafts.clear();
}
