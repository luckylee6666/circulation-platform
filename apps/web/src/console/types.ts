/** 与 Rust 侧 `#[serde(rename_all = "camelCase")]` 结构一一对应。 */

export interface ServiceStatus {
  running: boolean;
  port: number;
  startedAt: string | null;
  uptimeSeconds: number;
  dataDir: string;
  databaseSize: number;
  lastError: string | null;
}

export interface NetworkAddress {
  url: string;
  ip: string;
  interface: string;
  isPrimary: boolean;
}

export interface Settings {
  port: number;
  autostart: boolean;
  startServiceOnLaunch: boolean;
  minimizeToTray: boolean;
  dataDir: string | null;
}

export interface AppInfo {
  version: string;
  dataDir: string;
  databasePath: string;
  backupDir: string;
  logDir: string;
  settingsFile: string;
}

export interface BackupFile {
  fileName: string;
  path: string;
  size: number;
  createdAt: string;
}
