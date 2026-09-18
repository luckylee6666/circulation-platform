import type { ReactNode } from 'react';
import { Navigate, Outlet, useLocation } from 'react-router-dom';
import { Result, Spin } from 'antd';

import { useAuth } from './AuthProvider';
import ChangePasswordPage from '@/app/pages/ChangePasswordPage';

export function FullPageSpin({ tip = '加载中…' }: { tip?: string }) {
  return (
    <div className="full-page-center">
      <Spin size="large" description={tip} />
    </div>
  );
}

/** 未登录跳登录页；首次登录未改密则强制改密，改完才放行 */
export function RequireAuth() {
  const { user, loading, mustChangePassword } = useAuth();
  const location = useLocation();

  if (loading) {
    return <FullPageSpin />;
  }

  if (!user) {
    return <Navigate to="/login" state={{ from: location.pathname }} replace />;
  }

  if (mustChangePassword) {
    return <ChangePasswordPage forced />;
  }

  return <Outlet />;
}

export function RequirePermission({
  permission,
  children,
}: {
  permission: string;
  children: ReactNode;
}) {
  const { can } = useAuth();

  if (!can(permission)) {
    return (
      <Result
        status="403"
        title="没有访问权限"
        subTitle="当前账号的角色不包含该功能的权限，请联系管理员分配。"
      />
    );
  }

  return <>{children}</>;
}
