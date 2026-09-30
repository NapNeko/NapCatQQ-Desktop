// 输入框的草稿，按 Bot + 会话记在模块里：收起右栏、切走路由、换会话再回来，没发出去的字都还在。
// 只活在这次打开应用期间，不落盘。

import { EMPTY_DRAFT, type ComposerDraft } from '../../../core/domain/debug/composerModel';

const drafts = new Map<string, ComposerDraft>();

export function draftKey(botId: string, session: string): string {
    return `${botId}|${session}`;
}

export function readDraft(key: string): ComposerDraft {
    return drafts.get(key) ?? EMPTY_DRAFT;
}

export function writeDraft(key: string, draft: ComposerDraft): void {
    if (draft.text === '' && draft.mentions.length === 0) drafts.delete(key);
    else drafts.set(key, draft);
}

/** 测试用 */
export function _resetComposerDraftsForTests(): void {
    drafts.clear();
}
