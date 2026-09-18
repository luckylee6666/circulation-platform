import { useState } from 'react';
import { Alert, Button, Card, Form, Input, Typography } from 'antd';
import { LockOutlined, UserOutlined } from '@ant-design/icons';
import { Navigate, useLocation, useNavigate } from 'react-router-dom';

import { describeError } from '@/shared/api/client';
import { useAuth } from '@/shared/auth/AuthProvider';
import { useSiteName } from '@/shared/site/useSiteName';
import { FullPageSpin } from '@/shared/auth/RequireAuth';

interface FormValues {
  username: string;
  password: string;
}

export default function LoginPage() {
  const siteName = useSiteName();
  const { user, loading, login } = useAuth();
  const navigate = useNavigate();
  const location = useLocation();
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const from = (location.state as { from?: string } | null)?.from ?? '/dashboard';

  if (loading) {
    return <FullPageSpin tip="正在检查登录状态…" />;
  }

  if (user) {
    return <Navigate to={from} replace />;
  }

  const submit = async (values: FormValues) => {
    setSubmitting(true);
    setError(null);
    try {
      await login(values.username.trim(), values.password);
      navigate(from, { replace: true });
    } catch (err) {
      setError(describeError(err));
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div className="login-root">
      <Card className="login-card" variant="borderless">
        <div className="login-brand">
          <span className="login-mark" />
          <Typography.Title level={3} className="login-title">
            {siteName}
          </Typography.Title>
          <Typography.Text type="secondary">请使用管理员分配的账号登录</Typography.Text>
        </div>

        {error ? <Alert type="error" showIcon title={error} className="login-alert" /> : null}

        <Form<FormValues> layout="vertical" onFinish={submit} requiredMark={false} size="large">
          <Form.Item
            name="username"
            label="登录名"
            rules={[{ required: true, message: '请输入登录名' }]}
          >
            <Input prefix={<UserOutlined />} placeholder="登录名" autoComplete="username" autoFocus />
          </Form.Item>

          <Form.Item
            name="password"
            label="密码"
            rules={[{ required: true, message: '请输入密码' }]}
          >
            <Input.Password
              prefix={<LockOutlined />}
              placeholder="密码"
              autoComplete="current-password"
            />
          </Form.Item>

          <Button type="primary" htmlType="submit" block loading={submitting}>
            登录
          </Button>
        </Form>
      </Card>
    </div>
  );
}
