// 群号：从这个 Bot 加入的群里挑（搜群名或群号），也可以直接填。

import type { DebugTarget } from '../../../../core/ipc/generated/debug/DebugTarget';
import { ContactPicker } from './ContactPicker';
import type { FieldProps } from './fieldKit';

export function GroupPicker(props: FieldProps & { target: DebugTarget | null }) {
    return <ContactPicker {...props} kind="group" />;
}
