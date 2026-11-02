// 编辑框与提及弹层拥有同一组可访问标识；候选项只读取提及控制器。
import { useRef, type KeyboardEvent, type RefObject } from 'react';
import type { Contact, Draft } from '../../../core/domain/chat/model';
import { pruneMentions } from '../../../core/domain/debug/composerModel';
import { ChatAvatar } from '../../../shared/chat/ChatAvatar';
import { Popover, PopoverAnchor, PopoverContent } from '../../../shared/ui/Popover';
import type { ComposerDraftAccess } from './useComposerDraft';
import type { ComposerMentions } from './useComposerMentions';

export function ComposerEditor({
    input,
    contact,
    draft,
    editor,
    mentions,
    collapsed,
    onSend,
    onEscape,
}: {
    input: RefObject<HTMLTextAreaElement>;
    contact: Contact;
    draft: Draft;
    editor: ComposerDraftAccess;
    mentions: ComposerMentions;
    collapsed: boolean;
    onSend: () => void;
    onEscape: () => void;
}) {
    const composing = useRef(false);
    const onKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
        if (
            collapsed ||
            event.nativeEvent.isComposing ||
            composing.current ||
            event.keyCode === 229
        )
            return;
        if (event.key === 'Escape') {
            mentions.dismiss(true);
            onEscape();
            return;
        }
        if (mentions.handleKeyDown(event)) return;
        if (
            event.key === 'Enter' &&
            !event.shiftKey &&
            !event.altKey &&
            !event.ctrlKey &&
            !event.metaKey
        ) {
            event.preventDefault();
            onSend();
        }
    };
    const selected = mentions.matches[mentions.selected];
    return (
        <Popover
            open={mentions.visible}
            onOpenChange={(open) => {
                if (!open) mentions.dismiss(true);
            }}
        >
            <PopoverAnchor asChild>
                <div>
                    <textarea
                        ref={input}
                        aria-label={`发送消息给${contact.name}`}
                        placeholder="写点什么…"
                        value={draft.text}
                        rows={2}
                        aria-autocomplete={contact.type === 'group' ? 'list' : undefined}
                        aria-controls={mentions.visible ? mentions.listId : undefined}
                        aria-activedescendant={
                            mentions.visible && selected
                                ? `${mentions.listId}-${selected.id}`
                                : undefined
                        }
                        onChange={(event) => {
                            editor.patch({
                                text: event.target.value,
                                mentions: pruneMentions(event.target.value, draft.mentions ?? []),
                            });
                            mentions.setCaret(event.target.selectionStart);
                            mentions.dismiss(false);
                        }}
                        onSelect={(event) => mentions.setCaret(event.currentTarget.selectionStart)}
                        onCompositionStart={() => {
                            composing.current = true;
                        }}
                        onCompositionEnd={() => {
                            composing.current = false;
                        }}
                        onKeyDown={onKeyDown}
                    />
                </div>
            </PopoverAnchor>
            <PopoverContent
                side="top"
                align="start"
                className="native-chat-popover native-chat-mentions"
                onOpenAutoFocus={(event) => event.preventDefault()}
                onCloseAutoFocus={(event) => event.preventDefault()}
                onInteractOutside={(event) => {
                    if (event.detail.originalEvent.target === input.current) event.preventDefault();
                }}
                onEscapeKeyDown={() => mentions.dismiss(true)}
            >
                <div
                    ref={mentions.list}
                    id={mentions.listId}
                    role="listbox"
                    aria-label="群成员建议"
                >
                    {mentions.loading ? (
                        <p role="status">正在读取群成员…</p>
                    ) : mentions.error ? (
                        <>
                            <p role="status">{mentions.error}</p>
                            <button onClick={mentions.retry}>重试读取成员</button>
                        </>
                    ) : !mentions.matches.length ? (
                        <p role="status">没有匹配的成员</p>
                    ) : (
                        mentions.matches.map((member, index) => (
                            <button
                                key={member.id}
                                id={`${mentions.listId}-${member.id}`}
                                role="option"
                                aria-selected={index === mentions.selected}
                                tabIndex={-1}
                                onMouseDown={(event) => event.preventDefault()}
                                onClick={() => mentions.choose(member)}
                            >
                                <ChatAvatar
                                    contact={{
                                        type: 'private',
                                        id: member.id,
                                        name: member.name || member.id,
                                    }}
                                    small
                                />
                                <span className="min-w-0 flex-1 truncate">
                                    {member.name || member.id}
                                </span>
                                <span>{member.id}</span>
                            </button>
                        ))
                    )}
                </div>
            </PopoverContent>
        </Popover>
    );
}
