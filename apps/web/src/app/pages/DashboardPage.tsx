import { Card, Col, Descriptions, Row, Space, Tag, Typography } from 'antd';
import { useNavigate } from 'react-router-dom';

import { NAV_ITEMS, visibleNavItems } from '@/app/navigation';
import { useAuth } from '@/shared/auth/AuthProvider';

export default function DashboardPage() {
  const { user, roles, can } = useAuth();
  const navigate = useNavigate();

  const entries = visibleNavItems(NAV_ITEMS, can).flatMap((item) =>
    item.children ? item.children : [item],
  );

  return (
    <Space orientation="vertical" size="middle" style={{ width: '100%' }}>
      <Card>
        <Typography.Title level={4} className="dashboard-greeting">
          你好，{user?.displayName}
        </Typography.Title>
        <Typography.Paragraph type="secondary" className="dashboard-sub">
          欢迎使用流转平台。左侧菜单会按你的权限显示，没有权限的功能不会出现。
        </Typography.Paragraph>

        <Descriptions
          size="small"
          column={{ xs: 1, sm: 2 }}
          items={[
            { key: 'username', label: '登录名', children: user?.username ?? '—' },
            { key: 'dept', label: '部门', children: user?.dept || '—' },
            {
              key: 'roles',
              label: '角色',
              children: (
                <Space size={4} wrap>
                  {roles.map((role) => (
                    <Tag key={role} color="blue">
                      {role}
                    </Tag>
                  ))}
                </Space>
              ),
            },
            { key: 'lastLogin', label: '上次登录', children: user?.lastLoginAt ?? '首次登录' },
          ]}
        />
      </Card>

      <Card title="功能入口">
        <Row gutter={[16, 16]}>
          {entries.map((item) => (
            <Col key={item.key} xs={12} sm={8} md={6}>
              <button type="button" className="dashboard-entry" onClick={() => navigate(item.key)}>
                <span className="dashboard-entry-icon">{item.icon}</span>
                <span>{item.label}</span>
              </button>
            </Col>
          ))}
        </Row>
      </Card>
    </Space>
  );
}
