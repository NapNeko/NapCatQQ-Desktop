// 群成员请求、搜索和键盘选择只在提及弹层内维护；插入仍写入当前会话草稿。
import { useEffect, useId, useMemo, useRef, useState, type KeyboardEvent } from 'react';
import type { Contact } from '../../../core/domain/chat/model';
import { mentionLabel, mentionQueryAt } from '../../../core/domain/debug/composerModel';
import { errorText } from '../../../core/domain/errors';
import type { ChatAccountStore } from '../../../hooks/chat/chatStore';
import type { ComposerDraftAccess } from './useComposerDraft';

type Member = { id: string; name: string };

export function useComposerMentions({
    store,
    contact,
    text,
    editor,
    enabled,
}: {
    store: ChatAccountStore;
    contact: Contact;
    text: string;
    editor: ComposerDraftAccess;
    enabled: boolean;
}) {
    const [caret, setCaret] = useState(0);
    const [members, setMembers] = useState<Member[]>([]);
    const [error, setError] = useState('');
    const [loading, setLoading] = useState(false);
    const [dismissed, dismiss] = useState(false);
    const [index, setIndex] = useState(0);
    const [attempt, setAttempt] = useState(0);
    const loaded = useRef(false);
    const list = useRef<HTMLDivElement>(null);
    const listId = useId();
    const query = contact.type === 'group' && !dismissed ? mentionQueryAt(text, caret) : null;
    const visible = query !== null && enabled;
    useEffect(() => {
        if (!visible || loaded.current) return;
        let cancelled = false;
        setLoading(true);
        setError('');
        void store
            .members(contact.key)
            .then((value) => {
                if (!cancelled) {
                    setMembers(value);
                    loaded.current = true;
                }
            })
            .catch((error) => {
                if (!cancelled) setError(errorText(error));
            })
            .finally(() => {
                if (!cancelled) setLoading(false);
            });
        return () => {
            cancelled = true;
        };
    }, [visible, store, contact.key, attempt]);
    const search = useMemo(
        () =>
            members.map((member) => ({
                member,
                text: `${member.name} ${member.id}`.toLocaleLowerCase(),
            })),
        [members],
    );
    const needle = query?.query;
    const matches = useMemo(() => {
        if (needle === undefined) return [];
        const normalized = needle.toLocaleLowerCase();
        const result: Member[] = [];
        for (const entry of search) {
            if (!entry.text.includes(normalized)) continue;
            result.push(entry.member);
            if (result.length === 30) break;
        }
        return result;
    }, [needle, search]);
    const selected = Math.min(index, Math.max(0, matches.length - 1));
    useEffect(() => {
        setIndex(0);
    }, [needle]);
    useEffect(() => {
        list.current?.querySelector('[aria-selected=true]')?.scrollIntoView?.({ block: 'nearest' });
    }, [selected]);
    const choose = (member: Member) => {
        if (!query) return;
        const draft = editor.read();
        const label = mentionLabel(
            member.name,
            member.id,
            draft.mentions ?? [],
            members.filter((item) => item.name === member.name).length,
        );
        const nextText = draft.text.slice(0, query.start) + label + ' ' + draft.text.slice(caret);
        const nextCaret = query.start + label.length + 1;
        editor.commitText(nextText, nextCaret, [
            ...(draft.mentions ?? []),
            { qq: member.id, label },
        ]);
        setCaret(nextCaret);
        dismiss(true);
    };
    return {
        visible,
        list,
        listId,
        matches,
        selected,
        loading,
        error,
        choose,
        dismiss,
        setCaret,
        retry: () => setAttempt((value) => value + 1),
        insert: () => {
            dismiss(false);
            setCaret(editor.insert('@'));
        },
        handleKeyDown: (event: KeyboardEvent<HTMLTextAreaElement>) => {
            if (!visible) return false;
            if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
                event.preventDefault();
                if (matches.length)
                    setIndex(
                        (selected + (event.key === 'ArrowDown' ? 1 : -1) + matches.length) %
                            matches.length,
                    );
                return true;
            }
            if ((event.key === 'Enter' || event.key === 'Tab') && !event.shiftKey) {
                event.preventDefault();
                if (matches[selected]) choose(matches[selected]);
                else if (event.key === 'Tab') dismiss(true);
                return true;
            }
            return false;
        },
    };
}

export type ComposerMentions = ReturnType<typeof useComposerMentions>;
