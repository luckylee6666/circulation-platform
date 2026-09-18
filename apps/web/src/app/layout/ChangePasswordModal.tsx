import { Form, Input, Modal } from 'antd';
import { App as AntApp } from 'antd';

import { describeError } from '@/shared/api/client';
import { authApi } from '@/shared/api/endpoints';
import { useAuth } from '@/shared/auth/AuthProvider';

interface FormValues {
  oldPassword: string;
  newPassword: string;
  confirmPassword: string;
}

export default function ChangePasswordModal({
  open,
  onClose,
}: {
  open: boolean;
  onClose: () => void;
}) {
  const { message } = AntApp.useApp();
  const { reload } = useAuth();
  const [form] = Form.useForm<FormValues>();

  const submit = async () => {
    const values = await form.validateFields();
    try {
      await authApi.changePassword(values.oldPassword, values.newPassword);
      message.success('密码已修改，其它设备上的登录已失效');
      form.resetFields();
      onClose();
      await reload();
    } catch (error) {
      message.error(describeError(error));
    }
  };

  return (
    <Modal
      title="修改密码"
      open={open}
      onCancel={() => {
        form.resetFields();
        onClose();
      }}
      onOk={submit}
      okText="确认修改"
      cancelText="取消"
      destroyOnHidden
    >
      <Form form={form} layout="vertical" requiredMark={false}>
        <Form.Item
          name="oldPassword"
          label="当前密码"
          rules={[{ required: true, message: '请输入当前密码' }]}
        >
          <Input.Password autoComplete="current-password" />
        </Form.Item>

        <Form.Item
          name="newPassword"
          label="新密码"
          rules={[
            { required: true, message: '请输入新密码' },
            { min: 6, message: '新密码至少 6 位' },
          ]}
        >
          <Input.Password autoComplete="new-password" />
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
          <Input.Password autoComplete="new-password" />
        </Form.Item>
      </Form>
    </Modal>
  );
}
