import { describe, expect, it } from 'vitest';
import { describeSaveResult } from './save-result';

describe('Docker 配置保存反馈', () => {
    it('容器重建完成后明确告知端口和容器配置已应用', () => {
        const result = describeSaveResult('config_docker_recreated', '10001');
        expect(result.tone).toBe('success');
        expect(result.title).toContain('已保存');
        expect(result.content).toContain('10001');
        expect(result.content).toContain('容器');
        expect(result.content).toMatch(/生效|已应用|已更新/);
    });

    it('配置已保存但容器应用失败时给出重试启动的恢复动作', () => {
        const result = describeSaveResult('config_saved_docker_apply_failed', '10001');
        expect(result.tone).toBe('warning');
        expect(result.title).toContain('已保存');
        expect(result.content).toContain('10001');
        expect(`${result.title} ${result.content}`).toMatch(/失败|未能|未成功/);
        expect(result.content).toMatch(/重试.*启动|重新启动|再次启动/);
    });

    it('普通保存仍保留下次启动生效的反馈', () => {
        const result = describeSaveResult(null, '10001');
        expect(result.tone).toBe('success');
        expect(result.title).toContain('已保存');
        expect(result.content).toContain('下次启动');
    });
});
