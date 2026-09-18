import { Button, Card, Form, Input, Space, Typography, App as AntApp } from 'antd';
import { LockOutlined } from '@ant-design/icons';

import { describeError } from '@/shared/api/client';
import { authApi } from '@/shared/api/endpoints';
import { useAuth } from '@/shared/auth/AuthProvider';

interface FormValues {
  oldPassword: string;
  newPassword: string;
  confirmPassword: string;
}

/**
 * 首次登录（或被管理员重置密码后）必须先改密才能进入系统。
 * `forced` 为真时不带布局，也不给关闭入口。
 */
export default function ChangePasswordPage({ forced = false }: { forced?: boolean }) {
  const { message } = AntApp.useApp();
  const { user, logout, reload } = useAuth();
  const [form] = Form.useForm<FormValues>();

  const submit = async (values: FormValues) => {
    try {
      await authApi.changePassword(values.oldPassword, values.newPassword);
      message.success('密码已设置，正在进入系统');
      form.resetFields();
      await reload();
    } catch (error) {
      message.error(describeError(error));
    }
  };

  const body = (
    <>
      {forced ? (
        <Typography.Paragraph type="secondary" className="change-password-hint">
          {user?.displayName}，这是你第一次登录（或密码刚被重置），
          请先设置一个新密码。设置完成后会自动进入系统。
        </Typography.Paragraph>
      ) : null}

      <Form<FormValues> form={form} layout="vertical" onFinish={submit} requiredMark={false}>
        <Form.Item
          name="oldPassword"
          label="当前密码"
          rules={[{ required: true, message: '请输入当前密码' }]}
        >
          <Input.Password prefix={<LockOutlined />} autoComplete="current-password" />
        </Form.Item>

        <Form.Item
          name="newPassword"
          label="新密码"
          rules={[
            { required: true, message: '请输入新密码' },
            { min: 6, message: '新密码至少 6 位' },
          ]}
        >
          <Input.Password prefix={<LockOutlined />} autoComplete="new-password" />
        </Form.Item>

        <Form.Item
          name="confirmPassword"
          label="确认新密码"
          dependencies={['newPassword']}
          rules={[
            { required: true, message: '请再次输入新密码' },
            ({ getFieldValue }) => ({
              validator: (_, value) =>
                !value || getFieldValue('newPassword') === value
                  ? Promise.resolve()
                  : Promise.reject(new Error('两次输入的密码不一致')),
            }),
          ]}
        >
          <Input.Password prefix={<LockOutlined />} autoComplete="new-password" />
        </Form.Item>

        <Space>
          <Button type="primary" htmlType="submit">
            设置新密码
          </Button>
          <Button type="text" onClick={() => logout()}>
            退出登录
          </Button>
        </Space>
      </Form>
    </>
  );

  if (!forced) {
    return <Card title="修改密码">{body}</Card>;
  }

  return (
    <div className="login-root">
      <Card className="login-card" variant="borderless">
        <div className="login-brand">
          <span className="login-mark" />
          <Typography.Title level={4} className="login-title">
            设置新密码
          </Typography.Title>
        </div>
        {body}
      </Card>
    </div>
  );
}
