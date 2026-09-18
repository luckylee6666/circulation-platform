import type { ReactNode } from 'react';
import {
  BarChartOutlined,
  BellOutlined,
  DashboardOutlined,
  DatabaseOutlined,
  FileSearchOutlined,
  PartitionOutlined,
  SafetyCertificateOutlined,
  SettingOutlined,
  ShareAltOutlined,
  TagsOutlined,
  TeamOutlined,
  UploadOutlined,
} from '@ant-design/icons';

export interface NavItem {
  /** 叶子节点同时作为路由路径 */
  key: string;
  label: string;
  icon?: ReactNode;
  /** 需要该权限才可见 */
  permission?: string;
  /** 拥有其中任意一个权限即可见 */
  anyPermissions?: string[];
  children?: NavItem[];
}

export const NAV_ITEMS: NavItem[] = [
  { key: '/dashboard', label: '工作台', icon: <DashboardOutlined /> },
  {
    key: '/flows',
    label: '流转中心',
    icon: <ShareAltOutlined />,
    anyPermissions: ['flow:create', 'flow:dispatch', 'flow:handle', 'flow:confirm'],
  },
  {
    key: 'data',
    label: '数据管理',
    icon: <DatabaseOutlined />,
    children: [
      { key: '/records', label: '数据列表', permission: 'data:view' },
      { key: '/data/import', label: '导入数据', icon: <UploadOutlined />, permission: 'data:import' },
    ],
  },
  { key: '/stats', label: '统计', icon: <BarChartOutlined /> },
  { key: '/notifications', label: '消息中心', icon: <BellOutlined /> },
  {
    key: 'admin',
    label: '系统管理',
    icon: <SettingOutlined />,
    children: [
      { key: '/admin/users', label: '用户管理', icon: <TeamOutlined />, permission: 'user:manage' },
      {
        key: '/admin/roles',
        label: '角色权限',
        icon: <SafetyCertificateOutlined />,
        permission: 'role:manage',
      },
      { key: '/admin/fields', label: '字段定义', icon: <TagsOutlined />, permission: 'field:manage' },
      {
        key: '/admin/flows',
        label: '流程配置',
        icon: <PartitionOutlined />,
        permission: 'flow:def:manage',
      },
      {
        key: '/admin/audit',
        label: '审计日志',
        icon: <FileSearchOutlined />,
        permission: 'audit:view',
      },
    ],
  },
];

export function canSee(item: NavItem, can: (permission: string) => boolean): boolean {
  if (item.permission && !can(item.permission)) return false;
  if (item.anyPermissions && !item.anyPermissions.some(can)) return false;
  return true;
}

/** 过滤出当前账号可见的菜单，父级如果子项全被过滤掉也不显示 */
export function visibleNavItems(
  items: NavItem[],
  can: (permission: string) => boolean,
): NavItem[] {
  return items
    .map((item) => {
      if (!canSee(item, can)) return null;
      if (!item.children) return item;

      const children = visibleNavItems(item.children, can);
      return children.length > 0 ? { ...item, children } : null;
    })
    .filter((item): item is NavItem => item !== null);
}

/** 找到某个路径对应的菜单标题，用于页头面包屑 */
export function findNavTrail(items: NavItem[], pathname: string): NavItem[] {
  for (const item of items) {
    if (item.key === pathname) return [item];

    if (item.children) {
      const trail = findNavTrail(item.children, pathname);
      if (trail.length > 0) return [item, ...trail];
    }
  }
  return [];
}
