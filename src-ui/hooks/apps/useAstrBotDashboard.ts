import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { appFrameworkService } from '../../core/services/app-framework.service';
import { toAppConfigError } from '../../core/domain/apps/appConfigError';
import { pushAppErrorBar } from './pushAppErrorBar';
import type {
    AstrBotAbconfInfo,
    AstrBotDashboardStatus,
    AstrBotKbCreate,
    AstrBotKnowledgeBase,
    AstrBotPersona,
    AstrBotSessionRule,
} from '../../core/ipc/types';

export const astrbotDashboardKey = (id: string) => ['astrbotDashboard', id] as const;
export const astrbotPersonasKey = (id: string) => ['astrbotPersonas', id] as const;
export const astrbotKbsKey = (id: string) => ['astrbotKbs', id] as const;
export const astrbotRulesKey = (id: string) => ['astrbotRules', id] as const;
export const astrbotAbconfsKey = (id: string) => ['astrbotAbconfs', id] as const;
export const astrbotToolsKey = (id: string) => ['astrbotSubagentTools', id] as const;

function fail(title: string, key: string) {
    return (err: unknown) => {
        pushAppErrorBar({ key, title, raw: toAppConfigError(err).message });
    };
}

export function useAstrBotDashboardStatus(instanceId: string, enabled: boolean) {
    return useQuery<AstrBotDashboardStatus, Error>({
        queryKey: astrbotDashboardKey(instanceId),
        queryFn: () => appFrameworkService.astrbotDashboardStatus(instanceId),
        enabled,
        retry: false,
        staleTime: 10_000,
    });
}

export function useAstrBotPersonas(instanceId: string, enabled: boolean) {
    const qc = useQueryClient();
    const query = useQuery<AstrBotPersona[], Error>({
        queryKey: astrbotPersonasKey(instanceId),
        queryFn: () => appFrameworkService.astrbotListPersonas(instanceId),
        enabled,
        retry: false,
    });
    const upsert = useMutation({
        mutationFn: (args: { persona: AstrBotPersona; creating: boolean }) =>
            appFrameworkService.astrbotUpsertPersona(instanceId, args.persona, args.creating),
        onSuccess: (list) => qc.setQueryData(astrbotPersonasKey(instanceId), list),
        onError: fail('保存人格失败', `astrbot-persona:${instanceId}`),
    });
    const remove = useMutation({
        mutationFn: (personaId: string) =>
            appFrameworkService.astrbotDeletePersona(instanceId, personaId),
        onSuccess: (list) => qc.setQueryData(astrbotPersonasKey(instanceId), list),
        onError: fail('删除人格失败', `astrbot-persona-del:${instanceId}`),
    });
    return { ...query, upsert, remove };
}

export function useAstrBotKbs(instanceId: string, enabled: boolean) {
    const qc = useQueryClient();
    const query = useQuery<AstrBotKnowledgeBase[], Error>({
        queryKey: astrbotKbsKey(instanceId),
        queryFn: () => appFrameworkService.astrbotListKbs(instanceId),
        enabled,
        retry: false,
    });
    const create = useMutation({
        mutationFn: (req: AstrBotKbCreate) => appFrameworkService.astrbotCreateKb(instanceId, req),
        onSuccess: (list) => qc.setQueryData(astrbotKbsKey(instanceId), list),
        onError: fail('创建知识库失败', `astrbot-kb:${instanceId}`),
    });
    const remove = useMutation({
        mutationFn: (kbId: string) => appFrameworkService.astrbotDeleteKb(instanceId, kbId),
        onSuccess: (list) => qc.setQueryData(astrbotKbsKey(instanceId), list),
        onError: fail('删除知识库失败', `astrbot-kb-del:${instanceId}`),
    });
    return { ...query, create, remove };
}

export function useAstrBotSessionRules(instanceId: string, enabled: boolean) {
    const qc = useQueryClient();
    const query = useQuery<AstrBotSessionRule[], Error>({
        queryKey: astrbotRulesKey(instanceId),
        queryFn: () => appFrameworkService.astrbotListSessionRules(instanceId),
        enabled,
        retry: false,
    });
    const update = useMutation({
        mutationFn: (rule: AstrBotSessionRule) =>
            appFrameworkService.astrbotUpdateSessionRule(instanceId, rule),
        onSuccess: (list) => qc.setQueryData(astrbotRulesKey(instanceId), list),
        onError: fail('保存会话规则失败', `astrbot-rule:${instanceId}`),
    });
    const remove = useMutation({
        mutationFn: (args: { umo: string; ruleKey: string }) =>
            appFrameworkService.astrbotDeleteSessionRule(instanceId, args.umo, args.ruleKey),
        onSuccess: (list) => qc.setQueryData(astrbotRulesKey(instanceId), list),
        onError: fail('删除会话规则失败', `astrbot-rule-del:${instanceId}`),
    });
    return { ...query, update, remove };
}

export function useAstrBotAbconfs(instanceId: string, enabled: boolean) {
    const qc = useQueryClient();
    const query = useQuery<AstrBotAbconfInfo[], Error>({
        queryKey: astrbotAbconfsKey(instanceId),
        queryFn: () => appFrameworkService.astrbotListAbconfs(instanceId),
        enabled,
        retry: false,
    });
    const create = useMutation({
        mutationFn: (name: string) => appFrameworkService.astrbotCreateAbconf(instanceId, name),
        onSuccess: (list) => qc.setQueryData(astrbotAbconfsKey(instanceId), list),
        onError: fail('新建配置失败', `astrbot-abconf:${instanceId}`),
    });
    const remove = useMutation({
        mutationFn: (id: string) => appFrameworkService.astrbotDeleteAbconf(instanceId, id),
        onSuccess: (list) => qc.setQueryData(astrbotAbconfsKey(instanceId), list),
        onError: fail('删除配置失败', `astrbot-abconf-del:${instanceId}`),
    });
    return { ...query, create, remove };
}

export function useAstrBotSubagentTools(instanceId: string, enabled: boolean) {
    return useQuery<string[], Error>({
        queryKey: astrbotToolsKey(instanceId),
        queryFn: () => appFrameworkService.astrbotListSubagentTools(instanceId),
        enabled,
        retry: false,
        staleTime: 30_000,
    });
}
