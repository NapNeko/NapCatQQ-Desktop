import React, { useRef, useState } from 'react';
import {
  Button,
  Card,
  Text,
  Badge,
  Divider,
  Select,
} from '@fluentui/react-components';
import {
  DeleteRegular,
  PlayRegular,
  DatabaseRegular,
} from '@fluentui/react-icons';
import { useDiagnostics } from '../../hooks/diagnostics/useDiagnostics';
import { useEventStream, type UiEventRecord } from '../../hooks/diagnostics/useEventStream';
import { describeEvent } from '../../core/domain/events/event-label';

export const EventPanel: React.FC = () => {
  const { events, clear } = useEventStream();
  const { publishDemo } = useDiagnostics();
  const [filterKind, setFilterKind] = useState<string>('all');
  const [selectedEventId, setSelectedEventId] = useState<string | null>(null);
  const consoleBottomRef = useRef<HTMLDivElement>(null);

  const handleClear = () => {
    clear();
    setSelectedEventId(null);
  };

  const handleTriggerDemo = () => {
    void publishDemo();
  };

  const filteredEvents = events.filter((e) => filterKind === 'all' || e.kind === filterKind);
  const selectedEvent: UiEventRecord | undefined = events.find((e) => e.id === selectedEventId);

  return (
    <div className="panel-container">
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
        <div>
          <Text size={600} weight="semibold" style={{ color: '#242424' }}>
            系统事件总线流 (Events Hub)
          </Text>
          <Text size={200} block style={{ color: '#616161', marginTop: '4px' }}>
            监听并拦截来自 Tauri Rust 核心层分发的 Domain 事件，辅助诊断本地和远端部署实例。
          </Text>
        </div>
        <div style={{ display: 'flex', gap: '8px' }}>
          <Button icon={<PlayRegular />} onClick={handleTriggerDemo} appearance="secondary" size="small">
            发送测试事件
          </Button>
          <Button icon={<DeleteRegular />} onClick={handleClear} appearance="secondary" size="small">
            清空日志
          </Button>
        </div>
      </div>

      <div style={{ display: 'flex', gap: '20px', flex: 1, minHeight: '480px', overflow: 'hidden' }}>
        <div style={{ flex: 1.8, display: 'flex', flexDirection: 'column', gap: '12px' }}>
          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
            <Text weight="semibold" size={300}>核心事件流 (最新 100 条)</Text>
            <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
              <Text size={100} style={{ color: '#616161' }}>筛选类型:</Text>
              <Select value={filterKind} onChange={(e) => setFilterKind(e.target.value)} size="small">
                <option value="all">所有事件类型 (All)</option>
                <option value="bot_state_changed">状态转移 (bot_state_changed)</option>
                <option value="bot_status_changed">指标更新 (bot_status_changed)</option>
                <option value="bot_log_appended">日志追加 (bot_log_appended)</option>
                <option value="bot_error">异常报错 (bot_error)</option>
                <option value="task_progress">任务进度 (task_progress)</option>
              </Select>
            </div>
          </div>

          <div
            className="console-container"
            style={{
              flex: 1,
              maxHeight: 'unset',
              display: 'flex',
              flexDirection: 'column',
              gap: '6px',
            }}
          >
            {filteredEvents.length === 0 ? (
              <div style={{ flex: 1, display: 'flex', alignItems: 'center', justifyContent: 'center', color: '#858585', fontStyle: 'italic' }}>
                当前无捕获事件。您可以在左侧或本地/远端管理器进行操作，或点击"发送测试事件"。
              </div>
            ) : (
              filteredEvents.map((item) => {
                const desc = describeEvent(item.payload);
                return (
                  <div
                    key={item.id}
                    onClick={() => setSelectedEventId(item.id)}
                    style={{
                      padding: '8px 10px',
                      borderRadius: '4px',
                      cursor: 'pointer',
                      backgroundColor: selectedEventId === item.id ? '#37373d' : 'transparent',
                      display: 'flex',
                      flexDirection: 'column',
                      gap: '4px',
                      borderLeft: `3px solid ${item.kind === 'bot_error' ? '#bc2f32' : item.kind === 'bot_state_changed' ? '#0078d4' : '#858585'}`,
                    }}
                    className="event-row"
                  >
                    <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                      <div style={{ display: 'flex', gap: '8px', alignItems: 'center' }}>
                        <span className="console-line-time">[{item.timestamp}]</span>
                        <Badge size="small" color={desc.color}>
                          {desc.label}
                        </Badge>
                      </div>
                    </div>
                    <div
                      style={{
                        color:
                          item.kind === 'bot_error'
                            ? '#f48771'
                            : item.kind === 'task_progress'
                              ? '#cca700'
                              : '#dcdcdc',
                      }}
                      className="console-line"
                    >
                      {item.message}
                    </div>
                  </div>
                );
              })
            )}
            <div ref={consoleBottomRef} />
          </div>
        </div>

        <div style={{ flex: 1, display: 'flex', flexDirection: 'column' }}>
          <Text weight="semibold" size={300} style={{ marginBottom: '12px' }}>事件 Payload 解析器 (Debugger)</Text>
          {selectedEvent ? (
            <Card className="fluent-card" style={{ flex: 1, display: 'flex', flexDirection: 'column', gap: '12px' }}>
              <div>
                <Text size={300} weight="semibold">事件标识: {selectedEvent.id}</Text>
                <div style={{ display: 'flex', gap: '6px', marginTop: '4px' }}>
                  <Badge size="small" color={describeEvent(selectedEvent.payload).color}>
                    {selectedEvent.kind}
                  </Badge>
                  <Text size={100} style={{ color: '#858585' }}>捕获时间: {selectedEvent.timestamp}</Text>
                </div>
              </div>

              <Divider />

              <div style={{ flex: 1, overflowY: 'auto' }}>
                <Text weight="semibold" size={100} block style={{ color: '#616161', marginBottom: '6px' }}>
                  原始 JSON 结构
                </Text>
                <pre
                  style={{
                    margin: 0,
                    padding: '12px',
                    backgroundColor: '#1e1e1e',
                    color: '#9cdcfe',
                    borderRadius: '4px',
                    fontSize: '11px',
                    fontFamily: '"Cascadia Code", Consolas, monospace',
                    overflowX: 'auto',
                    whiteSpace: 'pre-wrap',
                  }}
                >
                  {JSON.stringify(selectedEvent.payload, null, 2)}
                </pre>
              </div>
            </Card>
          ) : (
            <Card className="fluent-card" style={{ flex: 1, display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
              <div style={{ textAlign: 'center', color: '#858585' }}>
                <DatabaseRegular style={{ fontSize: '36px', marginBottom: '8px' }} />
                <Text block size={200}>点击左侧事件项</Text>
                <Text size={100}>解析并审查其完整的序列化 Payload 数据结构</Text>
              </div>
            </Card>
          )}
        </div>
      </div>
    </div>
  );
};
