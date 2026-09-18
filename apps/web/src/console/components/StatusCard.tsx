import { CopyOutlined, ExportOutlined, PlayCircleOutlined, PoweroffOutlined } from '@ant-design/icons';
import { Alert, Button, Card, Descriptions, Space, Tag, Typography } from 'antd';

import { formatBytes, formatUptime } from '../format';
import type { ServiceStatus } from '../types';

const { Text } = Typography;

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

      {/*
        数据目录是个很长的路径，和另外三项挤在同一行时会把整排都撑到换行，
        「20 秒」「228.0 KB」这种简短的值也会被从中间断开。所以拆成两行：
        前三个短字段一行，路径单独占满一行。
      */}
      <Descriptions
        className="status-meta"
        size="small"
        column={1}
        items={[
          {
            key: 'brief',
            label: '运行状态',
            children: (
              <Space size={24} wrap>
                <span>
                  <Text type="secondary">运行时长 </Text>
                  {running ? formatUptime(status.uptimeSeconds) : '—'}
                </span>
                <span>
                  <Text type="secondary">启动时间 </Text>
                  {status.startedAt ?? '—'}
                </span>
                <span>
                  <Text type="secondary">数据大小 </Text>
                  {formatBytes(status.databaseSize)}
                </span>
              </Space>
            ),
          },
          {
            key: 'dir',
            label: '数据目录',
            children: (
              <Text className="status-dir" copyable={{ text: status.dataDir }}>
                {status.dataDir || '—'}
              </Text>
            ),
          },
        ]}
      />
    </Card>
  );
}
