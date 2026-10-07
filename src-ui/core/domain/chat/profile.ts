// 资料投影与成员权限判定的数据形状:只保留上游实际返回的字段,读取逻辑仍在 service 层。
import type { Contact } from './model';

export interface ProfileField {
    label: string;
    value: string;
}
export interface ChatProfile {
    name: string;
    fields: ProfileField[];
}
export interface ProfileMember extends Contact {
    nickname: string;
    role: string;
    title: string;
    joined?: string;
    lastSent?: string;
    mutedUntil?: number;
}
export type MemberPermission = {
    status: 'loading' | 'ready' | 'failed';
    member?: ProfileMember;
    expires: number;
    staleUntil?: number;
};
