import { CopyOutlined, ExportOutlined, PlayCircleOutlined, PoweroffOutlined } from '@ant-design/icons';
import { Alert, Button, Card, Descriptions, Space, Tag, Typography } from 'antd';

import { formatBytes, formatUptime } from '../format';
import type { ServiceStatus } from '../types';

interface Props {
  status: ServiceStatus;
  primaryUrl: string | null;
  busy: boolean;
  onStart: () => void;
  onStop: () => void;
  onOpenBrowser: () => void;
  onCopy: (text: string) => void;
}

export default function StatusCard({
  status,
  primaryUrl,
  busy,
  onStart,
  onStop,
  onOpenBrowser,
  onCopy,
}: Props) {
  const { running } = status;

  return (
    <Card>
      <div className="status-head">
        <span className={running ? 'status-dot status-dot-on' : 'status-dot status-dot-off'} />
        <div className="status-text">
          <Typography.Title level={4} className="status-title">
            {running ? '服务运行中' : '服务已停止'}
            {running && status.port ? (
              <Tag color="success" className="status-tag">
                端口 {status.port}
              </Tag>
            ) : null}
          </Typography.Title>
          <Typography.Text type="secondary" className="status-sub">
            {running
              ? primaryUrl ?? '暂时没有可用的局域网地址'
              : '启动后，局域网内的用户在浏览器里输入地址即可使用'}
          </Typography.Text>
        </div>
        <Space wrap className="status-actions">
          {running ? (
            <Button danger icon={<PoweroffOutlined />} loading={busy} onClick={onStop}>
              停止服务
            </Button>
          ) : (
            <Button type="primary" icon={<PlayCircleOutlined />} loading={busy} onClick={onStart}>
              启动服务
            </Button>
          )}
          <Button icon={<ExportOutlined />} disabled={!running || !primaryUrl} onClick={onOpenBrowser}>
            打开浏览器
          </Button>
          <Button
            icon={<CopyOutlined />}
            disabled={!primaryUrl}
            onClick={() => primaryUrl && onCopy(primaryUrl)}
          >
            复制地址
          </Button>
        </Space>
      </div>

      {status.lastError ? (
        <Alert type="error" showIcon className="status-alert" title={status.lastError} />
      ) : null}

      <Descriptions
        className="status-meta"
        size="small"
        column={{ xs: 1, sm: 2, md: 4 }}
        items={[
          { key: 'uptime', label: '运行时长', children: running ? formatUptime(status.uptimeSeconds) : '—' },
          { key: 'started', label: '启动时间', children: status.startedAt ?? '—' },
          { key: 'size', label: '数据大小', children: formatBytes(status.databaseSize) },
          { key: 'dir', label: '数据目录', children: status.dataDir || '—' },
        ]}
      />
    </Card>
  );
}
