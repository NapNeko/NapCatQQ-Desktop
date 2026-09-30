// 群成员：跟着同一张表单里的 group_id 拉成员列表；群号还没填时列表不可用，但输入框照样能填。

import type { DebugTarget } from '../../../../core/ipc/generated/debug/DebugTarget';
import { ContactPicker } from './ContactPicker';
import type { FieldProps } from './fieldKit';

export function MemberPicker(props: FieldProps & { target: DebugTarget | null; groupId: unknown }) {
    return <ContactPicker {...props} kind="member" />;
}
