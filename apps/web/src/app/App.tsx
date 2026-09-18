import { BrowserRouter, Navigate, Route, Routes } from 'react-router-dom';

import AppLayout from '@/app/layout/AppLayout';
import ChangePasswordPage from '@/app/pages/ChangePasswordPage';
import DashboardPage from '@/app/pages/DashboardPage';
import ImportPage from '@/app/pages/data/ImportPage';
import RecordsPage from '@/app/pages/data/RecordsPage';
import FlowCenterPage from '@/app/pages/flows/FlowCenterPage';
import InstanceDetailPage from '@/app/pages/flows/InstanceDetailPage';
import LoginPage from '@/app/pages/LoginPage';
import NotificationsPage from '@/app/pages/NotificationsPage';
import PlaceholderPage from '@/app/pages/PlaceholderPage';
import StatsPage from '@/app/pages/StatsPage';
import FieldsPage from '@/app/pages/admin/FieldsPage';
import FlowDefsPage from '@/app/pages/admin/FlowDefsPage';
import RolesPage from '@/app/pages/admin/RolesPage';
import UsersPage from '@/app/pages/admin/UsersPage';
import { AuthProvider } from '@/shared/auth/AuthProvider';
import { RequireAuth, RequirePermission } from '@/shared/auth/RequireAuth';

export default function App() {
  return (
    <BrowserRouter>
      <AuthProvider>
        <Routes>
          <Route path="/login" element={<LoginPage />} />

          <Route element={<RequireAuth />}>
            <Route element={<AppLayout />}>
              <Route index element={<Navigate to="/dashboard" replace />} />
              <Route path="/dashboard" element={<DashboardPage />} />

              <Route path="/flows" element={<FlowCenterPage />} />
              <Route path="/flows/:id" element={<InstanceDetailPage />} />

              <Route
                path="/records"
                element={
                  <RequirePermission permission="data:view">
                    <RecordsPage />
                  </RequirePermission>
                }
              />

              <Route
                path="/data/import"
                element={
                  <RequirePermission permission="data:import">
                    <ImportPage />
                  </RequirePermission>
                }
              />

              <Route path="/stats" element={<StatsPage />} />

              <Route
                path="/admin/users"
                element={
                  <RequirePermission permission="user:manage">
                    <UsersPage />
                  </RequirePermission>
                }
              />

              <Route
                path="/admin/roles"
                element={
                  <RequirePermission permission="role:manage">
                    <RolesPage />
                  </RequirePermission>
                }
              />

              <Route
                path="/admin/fields"
                element={
                  <RequirePermission permission="field:manage">
                    <FieldsPage />
                  </RequirePermission>
                }
              />

              <Route
                path="/admin/flows"
                element={
                  <RequirePermission permission="flow:def:manage">
                    <FlowDefsPage />
                  </RequirePermission>
                }
              />

              <Route path="/notifications" element={<NotificationsPage />} />

              <Route
                path="/admin/audit"
                element={
                  <RequirePermission permission="audit:view">
                    <PlaceholderPage
                      title="审计日志"
                      description="谁在什么时间做了什么操作，用于追溯。"
                    />
                  </RequirePermission>
                }
              />

              <Route path="/change-password" element={<ChangePasswordPage />} />
            </Route>
          </Route>

          <Route path="*" element={<Navigate to="/dashboard" replace />} />
        </Routes>
      </AuthProvider>
    </BrowserRouter>
  );
}
