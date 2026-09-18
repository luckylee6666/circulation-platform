import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { Button, Card, Empty, Segmented, Space, Spin, Tag, Typography } from 'antd';
import {
  BellOutlined,
  CheckCircleOutlined,
  RollbackOutlined,
  SendOutlined,
  StopOutlined,
} from '@ant-design/icons';

import type { NotificationItem } from '@/shared/api/types';
import { useNotify } from '@/shared/notify/NotifyProvider';

const { Text, Paragraph } = Typography;

const CATEGORY_STYLE: Record<string, { icon: React.ReactNode; color: string; label: string }> = {
  taskAssigned: { icon: <SendOutlined />, color: '#1d4ed8', label: '新待办' },
  taskRejected: { icon: <RollbackOutlined />, color: '#dc2626', label: '被打回' },
  flowFinished: { icon: <CheckCircleOutlined />, color: '#16a34a', label: '已办结' },
  flowTerminated: { icon: <StopOutlined />, color: '#6b7280', label: '已终止' },
};

export default function NotificationsPage() {
  const navigate = useNavigate();
  const { items, unreadCount, loading, markRead } = useNotify();
  const [filter, setFilter] = useState<'all' | 'unread'>('all');

  const visible = filter === 'unread' ? items.filter((item) => !item.readAt) : items;

  const open = async (item: NotificationItem) => {
    if (!item.readAt) await markRead([item.id]);
    if (item.refType === 'flow' && item.refId) {
      navigate(`/flows/${item.refId}`);
    }
  };

  return (
    <Card
      title="消息中心"
      extra={
        <Space>
          <Segmented
            value={filter}
            onChange={(value) => setFilter(value as 'all' | 'unread')}
            options={[
              { value: 'all', label: '全部' },
              { value: 'unread', label: `未读${unreadCount > 0 ? `（${unreadCount}）` : ''}` },
            ]}
          />
          <Button
            disabled={unreadCount === 0}
            onClick={() => markRead()}
          >
            全部已读
          </Button>
        </Space>
      }
    >
      {loading && items.length === 0 ? (
        <Spin description="加载中…" />
      ) : visible.length === 0 ? (
        <Empty
          image={Empty.PRESENTED_IMAGE_SIMPLE}
          description={filter === 'unread' ? '没有未读消息' : '还没有收到消息'}
        />
      ) : (
        <div className="notify-list">
          {visible.map((item) => {
            const style = CATEGORY_STYLE[item.category] ?? {
              icon: <BellOutlined />,
              color: '#6b7280',
              label: '提醒',
            };
            return (
              <button
                key={item.id}
                type="button"
                className={`notify-row${item.readAt ? '' : ' notify-row-unread'}`}
                onClick={() => open(item)}
              >
                <span className="notify-item-icon" style={{ color: style.color }}>
                  {style.icon}
                </span>
                <span className="notify-row-main">
                  <Space size={8} wrap>
                    <Tag>{style.label}</Tag>
                    <Text strong={!item.readAt}>{item.title}</Text>
                    {item.readAt ? null : <Tag color="red">未读</Tag>}
                  </Space>
                  <Paragraph type="secondary" style={{ margin: '4px 0 0' }}>
                    {item.content}
                  </Paragraph>
                </span>
                <Text type="secondary" className="notify-row-time">
                  {item.createdAt}
                </Text>
              </button>
            );
          })}
        </div>
      )}
    </Card>
  );
}
