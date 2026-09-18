import { useCallback, useEffect, useState } from 'react';
import { App as AntApp, Alert, Button, ConfigProvider, Space, Spin, Typography } from 'antd';
import { useQuery, useQueryClient } from '@tanstack/react-query';

import { api } from './api';
import AddressCard from './components/AddressCard';
import BrandingCard from './components/BrandingCard';
import LoadCard from './components/LoadCard';
import LogDrawer from './components/LogDrawer';
import MaintenanceCard from './components/MaintenanceCard';
import SettingsCard from './components/SettingsCard';
import StatusCard from './components/StatusCard';
import { isWindows } from './format';
import type { Settings } from './types';

export default function ConsoleApp() {
  const { message } = AntApp.useApp();
  const queryClient = useQueryClient();

  const [busy, setBusy] = useState(false);
  const [saving, setSaving] = useState(false);
  const [backupBusy, setBackupBusy] = useState(false);
  const [logOpen, setLogOpen] = useState(false);
  const [logs, setLogs] = useState('');

  const status = useQuery({
    queryKey: ['status'],
    queryFn: api.serviceStatus,
    refetchInterval: 2000,
  });

  const addresses = useQuery({
    queryKey: ['addresses'],
    queryFn: api.accessAddresses,
    refetchInterval: 5000,
  });

  const settings = useQuery({ queryKey: ['settings'], queryFn: api.getSettings });
  const info = useQuery({ queryKey: ['info'], queryFn: api.appInfo });
  const backups = useQuery({ queryKey: ['backups'], queryFn: api.listBackups });

  const primaryUrl =
    addresses.data?.find((item) => item.isPrimary)?.url ?? addresses.data?.[0]?.url ?? null;

  useEffect(() => {
    if (status.error) {
      message.error(`读取服务状态失败：${describe(status.error)}`);
    }
  }, [status.error, message]);

  const copy = useCallback(
    async (text: string) => {
      try {
        await api.copyText(text);
        message.success('访问地址已复制到剪贴板');
      } catch (error) {
        message.error(`复制失败：${describe(error)}`);
      }
    },
    [message],
  );

  const start = useCallback(async () => {
    setBusy(true);
    try {
      await api.serviceStart();
      message.success('服务已启动');
      await queryClient.invalidateQueries({ queryKey: ['status'] });
    } catch (error) {
      message.error(describe(error));
    } finally {
      setBusy(false);
    }
  }, [message, queryClient]);

  const stop = useCallback(async () => {
    setBusy(true);
    try {
      await api.serviceStop();
      message.success('服务已停止');
      await queryClient.invalidateQueries({ queryKey: ['status'] });
    } catch (error) {
      message.error(describe(error));
    } finally {
      setBusy(false);
    }
  }, [message, queryClient]);

  const save = useCallback(
    async (draft: Settings) => {
      setSaving(true);
      try {
        await api.saveSettings(draft);
        message.success('设置已保存');
        await queryClient.invalidateQueries({ queryKey: ['settings'] });
        await queryClient.invalidateQueries({ queryKey: ['status'] });
        await queryClient.invalidateQueries({ queryKey: ['addresses'] });
      } catch (error) {
        message.error(describe(error));
      } finally {
        setSaving(false);
      }
    },
    [message, queryClient],
  );

  const backupNow = useCallback(async () => {
    setBackupBusy(true);
    try {
      const result = await api.backupNow();
      message.success(`备份完成：${result.fileName}`);
      await queryClient.invalidateQueries({ queryKey: ['backups'] });
      await queryClient.invalidateQueries({ queryKey: ['status'] });
    } catch (error) {
      message.error(describe(error));
    } finally {
      setBackupBusy(false);
    }
  }, [message, queryClient]);

  const openLogs = useCallback(async () => {
    try {
      setLogs(await api.recentLogs(300));
      setLogOpen(true);
    } catch (error) {
      message.error(`读取日志失败：${describe(error)}`);
    }
  }, [message]);

  const openPath = useCallback(
    async (path: string) => {
      try {
        await api.openPath(path);
      } catch (error) {
        message.error(describe(error));
      }
    },
    [message],
  );

  if (status.isLoading || !status.data) {
    return (
      <div className="console-loading">
        <Spin size="large" tip="正在读取服务状态…">
          <span className="console-loading-inner" />
        </Spin>
      </div>
    );
  }

  return (
    <ConfigProvider theme={{ token: { colorPrimary: '#1d4ed8', borderRadius: 8 } }}>
      <div className="console-root">
        <header className="console-header">
          <Typography.Title level={3} className="console-title">
            流转平台
            <span className="console-subtitle">服务控制台</span>
          </Typography.Title>
          <Typography.Text type="secondary">
            版本 {info.data?.version ?? '—'}
            {api.isDesktop ? '' : ' · 浏览器预览模式（数据为模拟值）'}
          </Typography.Text>
        </header>

        <Space className="console-body" direction="vertical" size="middle" style={{ width: '100%' }}>
          <StatusCard
            status={status.data}
            primaryUrl={primaryUrl}
            busy={busy}
            onStart={start}
            onStop={stop}
            onOpenBrowser={() => primaryUrl && api.openUrl(primaryUrl)}
            onCopy={copy}
          />

          {isWindows() && status.data.running ? (
            <Alert
              type="warning"
              showIcon
              title="首次运行请在 Windows 防火墙提示中勾选「专用网络」并允许访问"
              description="如果同事打不开地址，多半是这一步被拦住了：可在「Windows 安全中心 → 防火墙和网络保护 → 允许应用通过防火墙」里放行本程序。"
            />
          ) : null}

          {status.data ? <LoadCard status={status.data} /> : null}

          <AddressCard
            addresses={addresses.data ?? []}
            primaryUrl={primaryUrl}
            onCopy={copy}
            onOpenBrowser={(url) => api.openUrl(url)}
          />

          <BrandingCard />

          {settings.data ? (
            <SettingsCard
              settings={settings.data}
              serviceRunning={status.data.running}
              saving={saving}
              onSave={save}
            />
          ) : null}

          <MaintenanceCard
            backups={backups.data ?? []}
            backupBusy={backupBusy}
            onBackup={backupNow}
            onOpenDataDir={() => openPath(info.data?.dataDir ?? status.data.dataDir)}
            onOpenLogs={openLogs}
            onRefreshBackups={() => queryClient.invalidateQueries({ queryKey: ['backups'] })}
            onReveal={(path) => api.revealPath(path).catch((error) => message.error(describe(error)))}
          />

          <Space className="console-footer">
            <Button type="text" danger onClick={() => api.quit()}>
              退出程序
            </Button>
            <Typography.Text type="secondary">
              关闭窗口不会停止服务，程序会收进系统托盘继续运行。
            </Typography.Text>
          </Space>
        </Space>
      </div>

      <LogDrawer open={logOpen} logs={logs} info={info.data ?? null} onClose={() => setLogOpen(false)} />
    </ConfigProvider>
  );
}

function describe(error: unknown): string {
  if (typeof error === 'string') return error;
  if (error instanceof Error) return error.message;
  return '未知错误';
}
