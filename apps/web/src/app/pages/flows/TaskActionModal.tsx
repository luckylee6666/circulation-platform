import { useEffect } from 'react';
import { App as AntApp, Form, Input, Modal, Select, Typography } from 'antd';
import { useMutation, useQuery } from '@tanstack/react-query';

import { describeError } from '@/shared/api/client';
import { flowsApi, usersApi } from '@/shared/api/endpoints';

const { Text } = Typography;

interface Props {
  open: boolean;
  taskId: number | null;
  /** dispatch / handle / confirm */
  stepType: string;
  action: 'complete' | 'pass' | 'reject';
  stepName: string;
  onClose: () => void;
  onDone: () => void;
}

interface FormValues {
  comment?: string;
  assignedTo?: number[];
}

const TITLES: Record<string, string> = {
  'dispatch:complete': '分派给承办人',
  'handle:complete': '提交承办结果',
  'confirm:pass': '确认通过',
  'confirm:reject': '打回重做',
};

export default function TaskActionModal({
  open,
  taskId,
  stepType,
  action,
  stepName,
  onClose,
  onDone,
}: Props) {
  const { message } = AntApp.useApp();
  const [form] = Form.useForm<FormValues>();

  const needsAssignees = stepType === 'dispatch';
  const needsComment = stepType !== 'dispatch';

  const users = useQuery({
    queryKey: ['user-options'],
    queryFn: usersApi.options,
    enabled: open && needsAssignees,
  });

  useEffect(() => {
    if (open) form.resetFields();
  }, [open, form]);

  const complete = useMutation({
    mutationFn: (values: FormValues) =>
      flowsApi.complete(taskId as number, {
        action,
        comment: values.comment?.trim() ?? '',
        assignedTo: values.assignedTo ?? [],
      }),
    onSuccess: () => {
      message.success(action === 'reject' ? '已打回' : '已提交');
      onClose();
      onDone();
    },
    onError: (error) => message.error(describeError(error)),
  });

  const submit = async () => {
    const values = await form.validateFields();
    complete.mutate(values);
  };

  return (
    <Modal
      open={open}
      title={TITLES[`${stepType}:${action}`] ?? '处理任务'}
      okText={action === 'reject' ? '确认打回' : '提交'}
      okButtonProps={{ danger: action === 'reject' }}
      cancelText="取消"
      confirmLoading={complete.isPending}
      onOk={submit}
      onCancel={onClose}
      destroyOnHidden
      width={520}
    >
      <Text type="secondary">
        当前环节：{stepName}
        {action === 'reject' ? '。打回后流程会退回到配置的步骤重新处理。' : ''}
      </Text>

      <Form form={form} layout="vertical" preserve={false} style={{ marginTop: 16 }}>
        {needsAssignees ? (
          <Form.Item
            name="assignedTo"
            label="指派给谁"
            rules={[{ required: true, message: '请至少选择一位承办人' }]}
          >
            <Select
              mode="multiple"
              placeholder="可以同时选择多人"
              loading={users.isLoading}
              optionFilterProp="label"
              options={(users.data ?? []).map((user) => ({
                value: user.id,
                label: user.dept ? `${user.displayName}（${user.dept}）` : user.displayName,
              }))}
            />
          </Form.Item>
        ) : null}

        <Form.Item
          name="comment"
          label={needsComment ? '处理说明' : '说明'}
          rules={
            action === 'reject' ? [{ required: true, message: '打回时请说明原因' }] : undefined
          }
        >
          <Input.TextArea
            rows={needsAssignees ? 2 : 4}
            placeholder={action === 'reject' ? '说明哪里需要重做' : '可以留空'}
            maxLength={500}
            showCount
          />
        </Form.Item>
      </Form>
    </Modal>
  );
}
