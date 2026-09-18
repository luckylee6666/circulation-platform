import type { AppInfo, BackupFile, NetworkAddress, ServiceStatus, Settings, LoadPoint } from './types';

/**
 * 控制台与 Rust 侧的桥。
 *
 * 不在 Tauri 里运行时（例如直接用 Vite 在浏览器打开 console.html 调界面），
 * 会走一组内存假数据，这样界面能脱离桌面壳独立开发与预览。
 */
const inTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!inTauri) {
    return mock<T>(command, args);
  }
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<T>(command, args);
}

const mockStartedAt = new Date(Date.now() - 193 * 60 * 1000);

/** 预览用的内存趋势。时间必须走真实进位，否则会算出 32:30 这种不存在的钟点。 */
function mockLoadHistory(points = 40): LoadPoint[] {
  const base = new Date(2026, 0, 1, 12, 0, 0);
  return Array.from({ length: points }, (_, index) => {
    const at = new Date(base.getTime() + index * 30 * 60 * 1000);
    return {
      at: `${String(at.getHours()).padStart(2, '0')}:${String(at.getMinutes()).padStart(2, '0')}`,
      memoryBytes: 48_000_000 + Math.round(Math.sin(index / 5) * 3_000_000) + index * 120_000,
    };
  });
}

const mockState = {
  running: true,
  siteName: '流转平台',
  port: 8080,
  // 与 Rust 侧一致：本地时间 'YYYY-MM-DD HH:MM:SS'
  startedAt: formatLocalDateTime(mockStartedAt),
  settings: {
    port: 8080,
    autostart: false,
    startServiceOnLaunch: true,
    minimizeToTray: false,
    dataDir: null,
  } as Settings,
};

function formatLocalDateTime(date: Date): string {
  const pad = (value: number) => String(value).padStart(2, '0');
  return (
    `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ` +
    `${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}`
  );
}

async function mock<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  await new Promise((resolve) => setTimeout(resolve, 80));

  switch (command) {
    case 'service_status':
      return {
        running: mockState.running,
        port: mockState.port,
        startedAt: mockState.startedAt,
        uptimeSeconds: mockState.running ? 193 * 60 : 0,
        dataDir: '/Users/you/Library/Application Support/com.byteflux.circulation',
        databaseSize: 13_021_184,
        lastError: null,
        memoryBytes: 52_428_800,
        cpuPercent: 1.2,
        onlineUsers: mockState.running ? 2 : 0,
        onlineSessions: mockState.running ? 5 : 0,
        loadHistory: mockState.running ? mockLoadHistory() : [],
      } satisfies ServiceStatus as T;

    case 'get_site_name':
      return (mockState.siteName ?? '流转平台') as T;

    case 'save_site_name': {
      const next = String((args as { name?: string } | undefined)?.name ?? '').trim();
      if (!next) throw new Error('平台名称不能为空');
      mockState.siteName = next;
      return next as T;
    }

    case 'service_start':
      mockState.running = true;
      return (await mock<T>('service_status')) as T;

    case 'service_stop':
      mockState.running = false;
      return (await mock<T>('service_status')) as T;

    case 'access_addresses':
      return [
        { url: `http://192.168.1.88:${mockState.port}`, ip: '192.168.1.88', interface: 'en0', isPrimary: true },
        { url: `http://10.8.0.3:${mockState.port}`, ip: '10.8.0.3', interface: 'utun3', isPrimary: false },
      ] satisfies NetworkAddress[] as T;

    case 'get_settings':
      return mockState.settings as T;

    case 'save_settings': {
      mockState.settings = args?.settings as Settings;
      mockState.port = mockState.settings.port;
      return mockState.settings as T;
    }

    case 'backup_now':
      return {
        fileName: 'circulation-20260918-214500.db',
        path: '/tmp/backups/circulation-20260918-214500.db',
        size: 13_021_184,
        createdAt: '2026-09-18 21:45:00',
      } satisfies BackupFile as T;

    case 'list_backups':
      return [] as BackupFile[] as T;

    case 'recent_logs':
      return [
        '2026-09-18T13:45:02.113Z  INFO 服务已启动，监听 0.0.0.0:8080',
        '2026-09-18T13:45:02.121Z  INFO 已应用数据库迁移 0001_init',
        '2026-09-18T13:46:11.402Z  INFO 登录成功 username=admin ip=192.168.1.23',
        '2026-09-18T13:47:55.887Z  WARN 登录失败：密码错误 username=zhangsan ip=192.168.1.31',
      ].join('\n') as T;

    case 'app_info':
      return {
        version: '0.1.0',
        dataDir: '/Users/you/Library/Application Support/com.byteflux.circulation',
        databasePath: '/Users/you/Library/Application Support/com.byteflux.circulation/data/circulation.db',
        backupDir: '/Users/you/Library/Application Support/com.byteflux.circulation/backups',
        logDir: '/Users/you/Library/Application Support/com.byteflux.circulation/logs',
        settingsFile: '/Users/you/Library/Application Support/com.byteflux.circulation/settings.json',
      } satisfies AppInfo as T;

    case 'copy_text':
      await navigator.clipboard?.writeText(String(args?.text ?? '')).catch(() => undefined);
      return undefined as T;

    default:
      return undefined as T;
  }
}

export const api = {
  getSiteName: () => call<string>('get_site_name'),
  saveSiteName: (name: string) => call<string>('save_site_name', { name }),
  isDesktop: inTauri,

  serviceStatus: () => call<ServiceStatus>('service_status'),
  serviceStart: (port?: number) => call<ServiceStatus>('service_start', { port }),
  serviceStop: () => call<ServiceStatus>('service_stop'),

  accessAddresses: () => call<NetworkAddress[]>('access_addresses'),

  getSettings: () => call<Settings>('get_settings'),
  saveSettings: (settings: Settings) => call<Settings>('save_settings', { settings }),

  backupNow: () => call<BackupFile>('backup_now'),
  listBackups: () => call<BackupFile[]>('list_backups'),
  recentLogs: (lines = 200) => call<string>('recent_logs', { lines }),
  appInfo: () => call<AppInfo>('app_info'),

  copyText: (text: string) => call<void>('copy_text', { text }),
  openUrl: (url: string) => call<void>('open_url', { url }),
  openPath: (path: string) => call<void>('open_path', { path }),
  revealPath: (path: string) => call<void>('reveal_path', { path }),
  quit: () => call<void>('quit_app'),
};
