import React from 'react';
import { FluentProvider, webLightTheme, Theme } from '@fluentui/react-components';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';

// Configure elegant light theme based on Windows Fluent principles
// Native light theme background is sometimes plain white; we customize to a slightly warmer neutral background (neutralLighter)
const customLightTheme: Theme = {
  ...webLightTheme,
  colorNeutralBackground1: '#f9f9fa', // Elegant subtle off-white with cool undertones
  colorNeutralBackground2: '#f3f3f4', // Slightly darker for headers and sidebars
  colorNeutralBackground3: '#ebebec',
  colorNeutralBackground4: '#e0e0e1',
  borderRadiusMedium: '6px', // 克制圆角
  borderRadiusLarge: '10px', // 主容器圆角
};

// Create QueryClient
const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      refetchOnWindowFocus: false,
      retry: false,
    },
  },
});

interface AppProvidersProps {
  children: React.ReactNode;
}

export const AppProviders: React.FC<AppProvidersProps> = ({ children }) => {
  return (
    <QueryClientProvider client={queryClient}>
      <FluentProvider theme={customLightTheme} style={{ minHeight: '100vh', display: 'flex', flexDirection: 'column' }}>
        {children}
      </FluentProvider>
    </QueryClientProvider>
  );
};
