import { useEffect, useMemo, useState } from 'react';
import { Avatar, Dropdown, Layout, Menu, Space, Tag, Typography } from 'antd';
import type { MenuProps } from 'antd';
import { DownOutlined, LogoutOutlined, UserOutlined } from '@ant-design/icons';
import { Outlet, useLocation, useNavigate } from 'react-router-dom';

import ChangePasswordModal from '@/app/layout/ChangePasswordModal';
import NotificationBell from '@/app/layout/NotificationBell';
import { findNavTrail, NAV_ITEMS, visibleNavItems, type NavItem } from '@/app/navigation';
import { useAuth } from '@/shared/auth/AuthProvider';
import { useSiteName } from '@/shared/site/useSiteName';
import { NotifyProvider } from '@/shared/notify/NotifyProvider';

type MenuItems = NonNullable<MenuProps['items']>;

/** 子菜单的 children 必须真的存在，所以只在有子项时才挂这个字段 */
function toMenuItems(items: NavItem[]): MenuItems {
  return items.map((item) => {
    const base = { key: item.key, icon: item.icon, label: item.label };
    return item.children ? { ...base, children: toMenuItems(item.children) } : base;
  });
}

export default function AppLayout() {
  const siteName = useSiteName();
  const { user, can, logout } = useAuth();
  const navigate = useNavigate();
  const location = useLocation();
  const [collapsed, setCollapsed] = useState(false);
  const [passwordOpen, setPasswordOpen] = useState(false);

  const menuItems = useMemo(() => visibleNavItems(NAV_ITEMS, can), [can]);
  const trail = useMemo(
    () => findNavTrail(menuItems, location.pathname),
    [menuItems, location.pathname],
  );

  // 进入子页面时自动展开对应的父级菜单
  const [openKeys, setOpenKeys] = useState<string[]>([]);
  useEffect(() => {
    const parent = trail.length > 1 ? trail[0].key : null;
    if (parent) {
      setOpenKeys((keys) => (keys.includes(parent) ? keys : [...keys, parent]));
    }
  }, [trail]);

  return (
    <NotifyProvider enabled={Boolean(user)}>
      <Layout className="app-layout">
        <Layout.Sider
          className="app-sider"
          theme="light"
          collapsible
          collapsed={collapsed}
          onCollapse={setCollapsed}
          width={216}
        >
          <div className="app-brand">
            <span className="app-brand-mark" />
            {collapsed ? null : <span className="app-brand-text">{siteName}</span>}
          </div>
          <Menu
            className="app-menu"
            mode="inline"
            selectedKeys={[location.pathname]}
            openKeys={openKeys}
            onOpenChange={setOpenKeys}
            items={toMenuItems(menuItems)}
            onClick={({ key }) => {
              if (key.startsWith('/')) navigate(key);
            }}
          />
        </Layout.Sider>

        <Layout>
          <Layout.Header className="app-header">
            <Typography.Text className="app-page-title">
              {trail.length > 0 ? trail[trail.length - 1].label : ''}
            </Typography.Text>

            <Space size={12}>
              {user?.dept ? <Tag variant="filled">{user.dept}</Tag> : null}
              <NotificationBell />
              <Dropdown
                trigger={['click']}
                menu={{
                  items: [
                    {
                      key: 'profile',
                      label: `${user?.displayName ?? ''}（${user?.username ?? ''}）`,
                      disabled: true,
                    },
                    { type: 'divider' },
                    { key: 'password', label: '修改密码', icon: <UserOutlined /> },
                    { key: 'logout', label: '退出登录', icon: <LogoutOutlined />, danger: true },
                  ],
                  onClick: async ({ key }) => {
                    if (key === 'password') setPasswordOpen(true);
                    if (key === 'logout') {
                      await logout();
                      navigate('/login', { replace: true });
                    }
                  },
                }}
              >
                <Space className="app-user" size={8}>
                  <Avatar size={30} style={{ backgroundColor: '#1d4ed8' }}>
                    {(user?.displayName ?? '?').slice(0, 1)}
                  </Avatar>
                  <span>{user?.displayName}</span>
                  <DownOutlined style={{ fontSize: 10, color: '#9aa1ab' }} />
                </Space>
              </Dropdown>
            </Space>
          </Layout.Header>

          <Layout.Content className="app-content">
            <Outlet />
          </Layout.Content>
        </Layout>

        <ChangePasswordModal open={passwordOpen} onClose={() => setPasswordOpen(false)} />
      </Layout>
    </NotifyProvider>
  );
}
