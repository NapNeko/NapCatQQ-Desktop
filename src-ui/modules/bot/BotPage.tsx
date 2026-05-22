import React, { useState } from 'react';
import { Button, Text } from '@fluentui/react-components';
import { BotListPage } from './list/BotListPage';
import { BotConfigPage } from './config/BotConfigPage';
import { BotLogPage } from './log/BotLogPage';

export const BotPage: React.FC = () => {
  const [view, setView] = useState<'list' | 'config' | 'log'>('list');
  const [selectedBotId, setSelectedBotId] = useState<string | null>(null);

  return (
    <div style={{ width: '100%', height: '100%', boxSizing: 'border-box', padding: '16px' }}>
      {view === 'list' && (
        <BotListPage
          onConfigureBot={(botId) => {
            setSelectedBotId(botId);
            setView('config');
          }}
          onViewLogs={(botId) => {
            setSelectedBotId(botId);
            setView('log');
          }}
        />
      )}
      {view === 'config' && (
        <BotConfigPage
          botId={selectedBotId}
          onBack={() => {
            setSelectedBotId(null);
            setView('list');
          }}
        />
      )}
      {view === 'log' && (
        selectedBotId ? (
          <BotLogPage
            botId={selectedBotId}
            onBack={() => {
              setSelectedBotId(null);
              setView('list');
            }}
          />
        ) : (
          <div style={{ padding: '24px', textAlign: 'center', backgroundColor: 'var(--ndf-bg-card)', border: '1px solid var(--ndf-border-subtle)', borderRadius: '8px' }}>
            <Text block style={{ marginBottom: '12px' }}>未选择要查看日志的 Bot 实例</Text>
            <Button onClick={() => setView('list')}>返回实例列表</Button>
          </div>
        )
      )}
    </div>
  );
};

export default BotPage;
