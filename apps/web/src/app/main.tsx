import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { App as AntApp, ConfigProvider } from 'antd';
import zhCN from 'antd/locale/zh_CN';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import 'antd/dist/reset.css';

import App from './App';
import './app.css';

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      // 接口错误已经带上可读文案，交给页面自己提示，自动重试只会让人困惑
      retry: false,
      refetchOnWindowFocus: false,
      staleTime: 10_000,
    },
  },
});

const container = document.getElementById('root');
if (!container) {
  throw new Error('页面缺少 #root 挂载点');
}

createRoot(container).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <ConfigProvider
        locale={zhCN}
        theme={{
          token: {
            colorPrimary: '#1d4ed8',
            borderRadius: 8,
          },
        }}
      >
        <AntApp>
          <App />
        </AntApp>
      </ConfigProvider>
    </QueryClientProvider>
  </StrictMode>,
);
