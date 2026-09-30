// QQ 号：从好友里挑（搜备注、昵称或号码），也可以直接填陌生人的号。

import type { DebugTarget } from '../../../../core/ipc/generated/debug/DebugTarget';
import { ContactPicker } from './ContactPicker';
import type { FieldProps } from './fieldKit';

export function FriendPicker(props: FieldProps & { target: DebugTarget | null }) {
    return <ContactPicker {...props} kind="friend" />;
}
