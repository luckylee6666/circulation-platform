import { CloudDownloadOutlined, FileTextOutlined, FolderOpenOutlined, ReloadOutlined } from '@ant-design/icons';
import { Button, Card, List, Space, Typography } from 'antd';

import { formatBytes } from '../format';
import type { BackupFile } from '../types';

interface Props {
  backups: BackupFile[];
  backupBusy: boolean;
  onBackup: () => void;
  onOpenDataDir: () => void;
  onOpenLogs: () => void;
  onRefreshBackups: () => void;
  onReveal: (path: string) => void;
}

export default function MaintenanceCard({
  backups,
  backupBusy,
  onBackup,
  onOpenDataDir,
  onOpenLogs,
  onRefreshBackups,
  onReveal,
}: Props) {
  return (
    <Card
      title="维护"
      extra={
        <Button size="small" type="text" icon={<ReloadOutlined />} onClick={onRefreshBackups}>
          刷新备份列表
        </Button>
      }
    >
      <Space wrap className="maintenance-actions">
        <Button type="primary" icon={<CloudDownloadOutlined />} loading={backupBusy} onClick={onBackup}>
          立即备份
        </Button>
        <Button icon={<FolderOpenOutlined />} onClick={onOpenDataDir}>
          打开数据目录
        </Button>
        <Button icon={<FileTextOutlined />} onClick={onOpenLogs}>
          查看运行日志
        </Button>
      </Space>

      <Typography.Title level={5} className="maintenance-title">
        最近的备份
      </Typography.Title>

      {backups.length === 0 ? (
        <Typography.Text type="secondary">
          还没有备份。建议定期点「立即备份」，把数据快照留在本机。
        </Typography.Text>
      ) : (
        <List
          size="small"
          dataSource={backups.slice(0, 5)}
          renderItem={(item) => (
            <List.Item
              actions={[
                <Button key="reveal" type="link" size="small" onClick={() => onReveal(item.path)}>
                  定位文件
                </Button>,
              ]}
            >
              <List.Item.Meta
                title={item.fileName}
                description={`${item.createdAt} · ${formatBytes(item.size)}`}
              />
            </List.Item>
          )}
        />
      )}
    </Card>
  );
}
