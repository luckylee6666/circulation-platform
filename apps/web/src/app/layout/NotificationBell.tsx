import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { Badge, Button, Empty, Popconfirm, Popover, Space, Spin, Typography } from 'antd';
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

const CATEGORY_STYLE: Record<string, { icon: React.ReactNode; color: string }> = {
  taskAssigned: { icon: <SendOutlined />, color: '#1d4ed8' },
  taskRejected: { icon: <RollbackOutlined />, color: '#dc2626' },
  flowFinished: { icon: <CheckCircleOutlined />, color: '#16a34a' },
  flowTerminated: { icon: <StopOutlined />, color: '#6b7280' },
};

export default function NotificationBell() {
  const navigate = useNavigate();
  const { items, unreadCount, loading, markRead, clearAll } = useNotify();
  const [open, setOpen] = useState(false);

  const openItem = async (item: NotificationItem) => {
    if (!item.readAt) {
      await markRead([item.id]);
    }
    setOpen(false);
    if (item.refType === 'flow' && item.refId) {
      navigate(`/flows/${item.refId}`);
    }
  };

  const content = (
    <div className="notify-panel">
      <div className="notify-panel-head">
        <Text strong>消息提醒</Text>
        <Space size={0}>
          <Button
            type="link"
            size="small"
            disabled={unreadCount === 0}
            onClick={() => markRead()}
          >
            全部已读
          </Button>
          <Popconfirm
            title="清空全部消息？"
            description="清空后无法恢复。"
            okText="清空"
            cancelText="取消"
            onConfirm={() => clearAll()}
          >
            <Button type="link" size="small" danger disabled={items.length === 0}>
              清空
            </Button>
          </Popconfirm>
        </Space>
      </div>

      <div className="notify-panel-body">
        {loading && items.length === 0 ? (
          <div className="notify-panel-empty">
            <Spin />
          </div>
        ) : items.length === 0 ? (
          <div className="notify-panel-empty">
            <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="暂无消息" />
          </div>
        ) : (
          items.map((item) => {
            const style = CATEGORY_STYLE[item.category] ?? { icon: <BellOutlined />, color: '#6b7280' };
            return (
              <button
                key={item.id}
                type="button"
                className={`notify-item${item.readAt ? '' : ' notify-item-unread'}`}
                onClick={() => openItem(item)}
              >
                <span className="notify-item-icon" style={{ color: style.color }}>
                  {style.icon}
                </span>
                <span className="notify-item-main">
                  <span className="notify-item-title">{item.title}</span>
                  <Paragraph className="notify-item-content" ellipsis={{ rows: 2 }}>
                    {item.content}
                  </Paragraph>
                  <Text type="secondary" className="notify-item-time">
                    {item.createdAt}
                  </Text>
                </span>
                {item.readAt ? null : <span className="notify-item-dot" />}
              </button>
            );
          })
        )}
      </div>

      <div className="notify-panel-foot">
        <Button
          type="link"
          size="small"
          block
          onClick={() => {
            setOpen(false);
            navigate('/notifications');
          }}
        >
          查看全部消息
        </Button>
      </div>
    </div>
  );

  return (
    <Popover
      open={open}
      onOpenChange={setOpen}
      trigger="click"
      placement="bottomRight"
      arrow={false}
      content={content}
      styles={{ content: { padding: 0 } }}
    >
      <span className="notify-trigger">
        <Badge count={unreadCount} size="small" overflowCount={99}>
          <Button type="text" icon={<BellOutlined />} aria-label="消息提醒" />
        </Badge>
      </span>
    </Popover>
  );
}
