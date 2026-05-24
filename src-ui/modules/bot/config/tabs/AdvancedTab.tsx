import React from 'react';
import {
  Field,
  Input,
  Checkbox,
  Dropdown,
  Option,
  Text,
  MessageBar,
  MessageBarBody,
} from '@fluentui/react-components';
import { AdvancedConfig } from '../../../../core/ipc/generated/domain/AdvancedConfig';
import { LogLevel } from '../../../../core/ipc/generated/domain/LogLevel';
import { BackendType } from '../../../../core/ipc/generated/domain/BackendType';

interface AdvancedTabProps {
  data: AdvancedConfig;
  onChange: (updated: Partial<AdvancedConfig>) => void;
  backendType: BackendType;
}

export const AdvancedTab: React.FC<AdvancedTabProps> = ({
  data,
  onChange,
  backendType,
}) => {
  const isSnowLuma = backendType === 'snowluma';

  const handleBypassChange = (key: keyof AdvancedConfig['bypass'], value: boolean) => {
    onChange({
      bypass: {
        ...data.bypass,
        [key]: value,
      },
    });
  };

  const logLevels: { value: LogLevel; label: string }[] = [
    { value: 'debug', label: 'Debug (全面排查调试)' },
    { value: 'info', label: 'Info (标准系统信息)' },
    { value: 'error', label: 'Error (仅输出错误日志)' },
  ];

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: '16px', maxWidth: '500px', padding: '12px 4px' }}>
      <div>
        <Text weight="semibold" size={300}>高阶与性能优化设置 (Advanced Engine Parameters)</Text>
      </div>

      {isSnowLuma && (
        <MessageBar intent="warning" style={{ width: '100%' }}>
          <MessageBarBody>
            当前实例底座为 <strong>SnowLuma</strong>。控制台日志文件、本地 O3 Hook 注入以及 NTQQ 反作弊沙箱规避等 NapCat 专有高阶特性在此运行环境下不生效，已在 UI 中自动隐藏。
          </MessageBarBody>
        </MessageBar>
      )}

      <div style={{ display: 'flex', flexDirection: 'column', gap: '8px' }}>
        <Checkbox
          label="系统开机自动运行 (Auto Start)"
          checked={data.autoStart}
          onChange={(_, val) => onChange({ autoStart: !!val.checked })}
        />

        <Checkbox
          label="掉线并下发通知 (Offline Notice)"
          checked={data.offlineNotice}
          onChange={(_, val) => onChange({ offlineNotice: !!val.checked })}
        />

        <Checkbox
          label="强制合并解析多媒体消息 (Parse Multi-Media Message)"
          checked={data.parseMultMsg}
          onChange={(_, val) => onChange({ parseMultMsg: !!val.checked })}
        />

        <Checkbox
          label="开启本地文件自动转换网络 URL 缓存 (Local File To URL)"
          checked={data.enableLocalFile2Url}
          onChange={(_, val) => onChange({ enableLocalFile2Url: !!val.checked })}
        />
      </div>

      {!isSnowLuma && (
        <>
          <div style={{ display: 'flex', flexDirection: 'column', gap: '12px', borderTop: '1px solid var(--ndf-border-subtle)', paddingTop: '16px' }}>
            <Text weight="semibold" size={200}>封包服务器设置 (Packet Servers)</Text>
            
            <Field label="封包上报转发服务器 (Packet Server Address)">
              <Input
                value={data.packetServer}
                onChange={(_, val) => onChange({ packetServer: val.value })}
                placeholder="http://my-packet-server.com:9000"
                style={{ width: '100%' }}
              />
            </Field>

            <Field label="封包过滤底层后端引擎 (Packet Backend Platform)">
              <Input
                value={data.packetBackend}
                onChange={(_, val) => onChange({ packetBackend: val.value })}
                placeholder="例如: default"
                style={{ width: '100%' }}
              />
            </Field>
          </div>

          <div style={{ display: 'flex', flexDirection: 'column', gap: '12px', borderTop: '1px solid var(--ndf-border-subtle)', paddingTop: '16px' }}>
            <Text weight="semibold" size={200}>控制台与日志策略 (Console Logging & Level)</Text>

            <Checkbox
              label="将终端流输出备份至本地文本文件 (Write File Log)"
              checked={data.fileLog}
              onChange={(_, val) => onChange({ fileLog: !!val.checked })}
            />

            <Checkbox
              label="控制台高亮终端实时打印 (Standard Console stdout)"
              checked={data.consoleLog}
              onChange={(_, val) => onChange({ consoleLog: !!val.checked })}
            />

            <Field label="本地文本日志等级 (File Log Level)">
              <Dropdown
                value={logLevels.find(l => l.value === data.fileLogLevel)?.label || data.fileLogLevel}
                selectedOptions={[data.fileLogLevel]}
                onOptionSelect={(_, val) => onChange({ fileLogLevel: val.optionValue as LogLevel })}
                style={{ width: '100%' }}
              >
                {logLevels.map((level) => (
                  <Option key={level.value} value={level.value}>
                    {level.label}
                  </Option>
                ))}
              </Dropdown>
            </Field>

            <Field label="控制台打印等级 (Console Log Level)">
              <Dropdown
                value={logLevels.find(l => l.value === data.consoleLogLevel)?.label || data.consoleLogLevel}
                selectedOptions={[data.consoleLogLevel]}
                onOptionSelect={(_, val) => onChange({ consoleLogLevel: val.optionValue as LogLevel })}
                style={{ width: '100%' }}
              >
                {logLevels.map((level) => (
                  <Option key={level.value} value={level.value}>
                    {level.label}
                  </Option>
                ))}
              </Dropdown>
            </Field>

            <Field label="O3 底层 Hook 注入模式 (O3 Hook Mode)">
              <Dropdown
                value={data.o3HookMode === 1 ? '开启 (Hook Injection On)' : '关闭 (Hook Injection Off)'}
                selectedOptions={[String(data.o3HookMode)]}
                onOptionSelect={(_, val) => onChange({ o3HookMode: Number(val.optionValue) })}
                style={{ width: '100%' }}
              >
                <Option value="1">开启 (Hook Injection On)</Option>
                <Option value="0">关闭 (Hook Injection Off)</Option>
              </Dropdown>
            </Field>
          </div>

          <div style={{ display: 'flex', flexDirection: 'column', gap: '8px', borderTop: '1px solid var(--ndf-border-subtle)', paddingTop: '16px' }}>
            <Text weight="semibold" size={200}>核心反检测防护与防沙箱规避 (Bypass Mitigations)</Text>
            <Text block size={100} style={{ color: 'var(--colorNeutralForeground4)', marginBottom: '8px' }}>
              安全防沙箱与反注入环境伪装。启用这些选项能使底层模块更难被 QQ 官方的反作弊沙箱截获。
            </Text>

            <Checkbox
              label="绕过内存 Hook 检测 (Bypass Hook Detection)"
              checked={data.bypass.hook}
              onChange={(_, val) => handleBypassChange('hook', !!val.checked)}
            />

            <Checkbox
              label="绕过窗口特征扫描 (Bypass Window Feature Detection)"
              checked={data.bypass.window}
              onChange={(_, val) => handleBypassChange('window', !!val.checked)}
            />

            <Checkbox
              label="伪装系统 DLL 模块加载 (Bypass Module Load Scanning)"
              checked={data.bypass.module}
              onChange={(_, val) => handleBypassChange('module', !!val.checked)}
            />

            <Checkbox
              label="进程环境隐藏 (Bypass Parent/Sibling Process Traversal)"
              checked={data.bypass.process}
              onChange={(_, val) => handleBypassChange('process', !!val.checked)}
            />

            <Checkbox
              label="物理容器沙箱混淆 (Bypass App Container Isolation)"
              checked={data.bypass.container}
              onChange={(_, val) => handleBypassChange('container', !!val.checked)}
            />

            <Checkbox
              label="JS 运行引擎加壳混淆 (Bypass JS Runtime Signature)"
              checked={data.bypass.js}
              onChange={(_, val) => handleBypassChange('js', !!val.checked)}
            />
          </div>
        </>
      )}
    </div>
  );
};
