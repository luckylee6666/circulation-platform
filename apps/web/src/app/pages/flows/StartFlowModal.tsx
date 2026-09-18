import { useEffect, useState } from 'react';
import { App as AntApp, Form, Input, Modal, Select, Space, Typography } from 'antd';
import { useMutation, useQuery } from '@tanstack/react-query';

import { describeError } from '@/shared/api/client';
import { flowsApi, recordsApi } from '@/shared/api/endpoints';
import type { RecordItem } from '@/shared/api/types';

const { Text } = Typography;

interface Props {
  open: boolean;
  onClose: () => void;
  onCreated: (instanceId: number) => void;
  /** 从数据列表发起时带上记录 */
  record?: RecordItem | null;
}

interface FormValues {
  defId: number;
  title: string;
  recordId?: number;
}

export default function StartFlowModal({ open, onClose, onCreated, record }: Props) {
  const { message } = AntApp.useApp();
  const [form] = Form.useForm<FormValues>();
  const [recordKeyword, setRecordKeyword] = useState('');
  const [selectedRecord, setSelectedRecord] = useState<RecordItem | null>(record ?? null);

  const defs = useQuery({
    queryKey: ['flow-defs'],
    queryFn: flowsApi.defs,
    enabled: open,
  });

  const records = useQuery({
    queryKey: ['records', 'picker', recordKeyword],
    queryFn: () => recordsApi.list(recordKeyword, 1, 20),
    enabled: open && !record,
  });

  // 打开时把默认流程和标题预填好，少点几下
  useEffect(() => {
    if (!open) return;
    setSelectedRecord(record ?? null);
    const preferred = defs.data?.find((item) => item.isDefault) ?? defs.data?.[0];
    form.setFieldsValue({
      defId: preferred?.id,
      title: record ? String(record.data.code ?? record.data.name ?? '') : '',
      recordId: record?.id,
    });
  }, [open, defs.data, record, form]);

  const start = useMutation({
    mutationFn: (values: FormValues) =>
      flowsApi.start({
        defId: values.defId,
        title: values.title.trim(),
        recordId: values.recordId ?? null,
        form: selectedRecord?.data ?? {},
      }),
    onSuccess: (result) => {
      message.success('已发起，下一步的待办已推送');
      form.resetFields();
      onClose();
      onCreated(result.id);
    },
    onError: (error) => message.error(describeError(error)),
  });

  const submit = async () => {
    const values = await form.validateFields();
    start.mutate(values);
  };

  return (
    <Modal
      open={open}
      title="发起流转"
      okText="发起"
      cancelText="取消"
      confirmLoading={start.isPending}
      onOk={submit}
      onCancel={onClose}
      destroyOnHidden
      width={560}
    >
      <Form form={form} layout="vertical" preserve={false}>
        <Form.Item
          name="defId"
          label="流程"
          rules={[{ required: true, message: '请选择流程' }]}
        >
          <Select
            placeholder="选择要走的流程"
            options={(defs.data ?? [])
              .filter((item) => item.enabled)
              .map((item) => ({
                value: item.id,
                label: item.name,
              }))}
          />
        </Form.Item>

        <Form.Item
          name="title"
          label="事项标题"
          rules={[{ required: true, message: '请填写标题' }]}
        >
          <Input placeholder="一句话说明这是什么事项" maxLength={120} />
        </Form.Item>

        {record ? (
          <Form.Item label="关联数据">
            <Space orientation="vertical" size={2}>
              <Text>{describeRecord(record)}</Text>
              <Text type="secondary">该记录的字段会一起带进流程，可用于条件判断</Text>
            </Space>
          </Form.Item>
        ) : (
          <Form.Item name="recordId" label="关联数据（可选）">
            <Select
              allowClear
              showSearch
              placeholder="搜索并关联一条数据"
              filterOption={false}
              onSearch={setRecordKeyword}
              onChange={(value) => {
                const found = records.data?.items.find((item) => item.id === value) ?? null;
                setSelectedRecord(found);
              }}
              options={(records.data?.items ?? []).map((item) => ({
                value: item.id,
                label: describeRecord(item),
              }))}
              notFoundContent={records.isLoading ? '搜索中…' : '没有匹配的数据'}
            />
          </Form.Item>
        )}
      </Form>
    </Modal>
  );
}

/** 用数据里第一个有值的字段拼一个可读的标签。 */
export function describeRecord(record: RecordItem): string {
  const values = Object.values(record.data).filter(
    (value) => value !== null && value !== undefined && String(value).trim() !== '',
  );
  const head = values.slice(0, 3).join(' · ');
  return head || `记录 #${record.id}`;
}
