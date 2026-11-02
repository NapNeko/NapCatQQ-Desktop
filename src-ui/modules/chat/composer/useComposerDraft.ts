// 草稿读写绑定会话；异步附件完成时合并最新草稿，不覆盖期间输入的文字。
import { useMemo, type RefObject } from 'react';
import { EMPTY_DRAFT, type Draft, type SessionKey } from '../../../core/domain/chat/model';
import { pruneMentions } from '../../../core/domain/debug/composerModel';
import { useChatSelector, type ChatAccountStore } from '../../../hooks/chat/chatStore';
import { useCommittedCaret } from '../../../hooks/ui/useCommittedCaret';

export interface ComposerDraftAccess {
    read: () => Draft;
    patch: (value: Partial<Draft>) => void;
    commitText: (text: string, caret: number, mentions: Draft['mentions']) => void;
    insert: (value: string) => number;
}

export function useComposerDraft(
    store: ChatAccountStore,
    session: SessionKey,
    input: RefObject<HTMLTextAreaElement>,
) {
    const draft = useChatSelector(
        store,
        (snapshot) => snapshot.account.drafts[session] ?? EMPTY_DRAFT,
    );
    const commitCaret = useCommittedCaret(input, draft.text);
    const editor = useMemo<ComposerDraftAccess>(() => {
        const read = () => store.getSnapshot().account.drafts[session] ?? EMPTY_DRAFT;
        const patch = (value: Partial<Draft>) => store.draft(session, { ...read(), ...value });
        const commitText = (text: string, caret: number, mentions: Draft['mentions']) => {
            commitCaret(text, caret);
            patch({ text, mentions });
        };
        return {
            read,
            patch,
            commitText,
            insert: (value) => {
                const current = read();
                const start = input.current?.selectionStart ?? current.text.length;
                const end = input.current?.selectionEnd ?? start;
                const text = current.text.slice(0, start) + value + current.text.slice(end);
                const caret = start + value.length;
                commitText(text, caret, pruneMentions(text, current.mentions ?? []));
                return caret;
            },
        };
    }, [store, session, input, commitCaret]);
    return { draft, editor };
}
