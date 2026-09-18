import { CopyOutlined, ExportOutlined } from '@ant-design/icons';
import { Button, Card, Empty, Space, Table, Tag, Typography } from 'antd';
import type { TableColumnsType } from 'antd';
import { QRCodeSVG } from 'qrcode.react';

import type { NetworkAddress } from '../types';

interface Props {
  addresses: NetworkAddress[];
  primaryUrl: string | null;
  onCopy: (text: string) => void;
  onOpenBrowser: (url: string) => void;
}

export default function AddressCard({ addresses, primaryUrl, onCopy, onOpenBrowser }: Props) {
  const columns: TableColumnsType<NetworkAddress> = [
    {
      title: '访问地址',
      dataIndex: 'url',
      key: 'url',
      render: (url: string, record) => (
        <Space>
          <Typography.Text strong={record.isPrimary} copyable={false}>
            {url}
          </Typography.Text>
          {record.isPrimary ? <Tag color="blue">主地址</Tag> : null}
        </Space>
      ),
    },
    {
      title: '网卡',
      dataIndex: 'interface',
      key: 'interface',
      width: 120,
      render: (name: string) => <Typography.Text type="secondary">{name}</Typography.Text>,
    },
    {
      title: '操作',
      key: 'actions',
      width: 180,
      render: (_, record) => (
        <Space>
          <Button size="small" icon={<CopyOutlined />} onClick={() => onCopy(record.url)}>
            复制
          </Button>
          <Button
            size="small"
            type="link"
            icon={<ExportOutlined />}
            onClick={() => onOpenBrowser(record.url)}
          >
            打开
          </Button>
        </Space>
      ),
    },
  ];

  return (
    <Card title="局域网访问地址" className="address-card">
      {addresses.length === 0 ? (
        <Empty description="未检测到可用的局域网地址，请确认服务器已连接内网" />
      ) : (
        <div className="address-body">
          <Table
            className="address-table"
            rowKey="url"
            size="medium"
            pagination={false}
            columns={columns}
            dataSource={addresses}
          />
          <div className="address-qr">
            {primaryUrl ? <QRCodeSVG value={primaryUrl} size={148} level="M" /> : null}
            <Typography.Text type="secondary" className="address-qr-hint">
              手机扫码直接打开
            </Typography.Text>
          </div>
        </div>
      )}
    </Card>
  );
}
