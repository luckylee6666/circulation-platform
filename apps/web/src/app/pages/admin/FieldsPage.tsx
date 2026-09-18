import { useMemo, useState } from 'react';
import {
  Alert,
  App as AntApp,
  Button,
  Card,
  Form,
  Input,
  InputNumber,
  Modal,
  Select,
  Space,
  Switch,
  Table,
  Tag,
  Typography,
} from 'antd';
import type { TableColumnsType } from 'antd';
import { PlusOutlined, ReloadOutlined } from '@ant-design/icons';
import { useQuery, useQueryClient } from '@tanstack/react-query';

import { describeError } from '@/shared/api/client';
import { fieldsApi } from '@/shared/api/endpoints';
import type { FieldDef, FieldInput, FieldType } from '@/shared/api/types';

const FIELD_TYPES: { value: FieldType; label: string }[] = [
  { value: 'text', label: '文本' },
  { value: 'number', label: '数字' },
  { value: 'date', label: '日期' },
  { value: 'select', label: '下拉选项' },
  { value: 'bool', label: '是 / 否' },
];

export default function FieldsPage() {
  const { message, modal } = AntApp.useApp();
  const queryClient = useQueryClient();
  const [form] = Form.useForm<FieldInput>();

  const [editing, setEditing] = useState<FieldDef | null>(null);
  const [open, setOpen] = useState(false);
  const [saving, setSaving] = useState(false);
  const fieldType = Form.useWatch('fieldType', form);

  const fields = useQuery({ queryKey: ['fields'], queryFn: fieldsApi.list });

  const uniqueKey = useMemo(
    () => (fields.data ?? []).find((item) => item.isUniqueKey),
    [fields.data],
  );

  const refresh = () => queryClient.invalidateQueries({ queryKey: ['fields'] });

  const submit = async () => {
    const values = await form.validateFields();
    setSaving(true);
    try {
      const payload: FieldInput = {
        ...values,
        options: values.options ?? [],
      };

      if (editing) {
        await fieldsApi.update(editing.id, payload);
        message.success('字段已更新');
      } else {
        await fieldsApi.create(payload);
        message.success('字段已新增');
      }
      setOpen(false);
      setEditing(null);
      await refresh();
    } catch (error) {
      message.error(describeError(error));
    } finally {
      setSaving(false);
    }
  };

  const remove = (field: FieldDef) => {
    modal.confirm({
      title: `删除字段「${field.label}」？`,
      content: '如果这个字段下已经有数据，删除会被拒绝，可以改为「停用」。',
      okText: '删除',
      okButtonProps: { danger: true },
      cancelText: '取消',
      onOk: async () => {
        try {
          await fieldsApi.remove(field.id);
          message.success('已删除');
          await refresh();
        } catch (error) {
          message.error(describeError(error));
        }
      },
    });
  };

  const columns: TableColumnsType<FieldDef> = [
    {
      title: '字段',
      key: 'label',
      render: (_, record) => (
        <Space orientation="vertical" size={0}>
          <Space size={6}>
            <Typography.Text strong>{record.label}</Typography.Text>
            {record.isUniqueKey ? <Tag color="blue">唯一键</Tag> : null}
            {record.required ? <Tag>必填</Tag> : null}
            {record.enabled ? null : <Tag color="default">已停用</Tag>}
          </Space>
          <Typography.Text type="secondary" style={{ fontSize: 12 }}>
            {record.code}
          </Typography.Text>
        </Space>
      ),
    },
    {
      title: '类型',
      dataIndex: 'fieldType',
      key: 'fieldType',
      width: 110,
      render: (value: FieldType) =>
        FIELD_TYPES.find((item) => item.value === value)?.label ?? value,
    },
    {
      title: '选项',
      dataIndex: 'options',
      key: 'options',
      render: (options: string[], record) =>
        record.fieldType === 'select' ? (
          <Space size={4} wrap>
            {options.map((option) => (
              <Tag key={option} variant="filled">
                {option}
              </Tag>
            ))}
          </Space>
        ) : (
          '—'
        ),
    },
    { title: '排序', dataIndex: 'sort', key: 'sort', width: 80 },
    {
      title: '操作',
      key: 'actions',
      width: 140,
      render: (_, record) => (
        <Space size={4}>
          <Button
            type="link"
            size="small"
            onClick={() => {
              setEditing(record);
              setOpen(true);
            }}
          >
            编辑
          </Button>
          <Button type="link" size="small" danger onClick={() => remove(record)}>
            删除
          </Button>
        </Space>
      ),
    },
  ];

  return (
    <Card
      title="字段定义"
      extra={
        <Space>
          <Button icon={<ReloadOutlined />} onClick={refresh}>
            刷新
          </Button>
          <Button
            type="primary"
            icon={<PlusOutlined />}
            onClick={() => {
              setEditing(null);
              setOpen(true);
            }}
          >
            新增字段
          </Button>
        </Space>
      }
    >
      <Typography.Paragraph type="secondary">
        这里定义的是「从外部系统导入的数据长什么样」。导入时按这些字段做列映射和校验，
        所以改字段之前请先确认线上数据不会受影响。
      </Typography.Paragraph>

      {uniqueKey ? (
        <Alert
          type="info"
          showIcon
          className="fields-alert"
          title={`当前唯一键字段：${uniqueKey.label}`}
          description="导入时用它对数据判重，从而实现「按编号覆盖更新」。全库只能有一个。"
        />
      ) : (
        <Alert
          type="warning"
          showIcon
          className="fields-alert"
          title="还没有设置唯一键字段"
          description="没有唯一键就无法判断数据是否重复，导入只能全部新增。建议把「编号」一类的字段标记为唯一键。"
        />
      )}

      <Table
        rowKey="id"
        size="medium"
        loading={fields.isLoading}
        columns={columns}
        dataSource={fields.data ?? []}
        pagination={false}
      />

      <Modal
        title={editing ? `编辑字段：${editing.label}` : '新增字段'}
        open={open}
        onCancel={() => {
          setOpen(false);
          setEditing(null);
        }}
        onOk={submit}
        confirmLoading={saving}
        okText="保存"
        cancelText="取消"
        destroyOnHidden
      >
        <Form<FieldInput>
          form={form}
          layout="vertical"
          preserve={false}
          initialValues={
            editing
              ? { ...editing }
              : {
                  fieldType: 'text' as FieldType,
                  required: false,
                  isUniqueKey: false,
                  options: [],
                  sort: 0,
                  showInList: true,
                  searchable: true,
                  enabled: true,
                }
          }
        >
          <Form.Item
            name="label"
            label="字段名称"
            rules={[{ required: true, message: '请输入字段名称' }]}
            extra="会显示在列表表头和导出文件的表头里"
          >
            <Input placeholder="如：编号" />
          </Form.Item>

          <Form.Item
            name="code"
            label="字段标识"
            rules={[
              { required: true, message: '请输入字段标识' },
              {
                pattern: /^[A-Za-z0-9_-]+$/,
                message: '只能用英文字母、数字、下划线或短横线',
              },
            ]}
            extra={editing ? '已有数据的字段不能改标识' : '英文，如 code、part_no'}
          >
            <Input placeholder="如：code" />
          </Form.Item>

          <Form.Item
            name="fieldType"
            label="类型"
            rules={[{ required: true }]}
            extra="决定导入时怎么校验这一列的值"
          >
            <Select options={FIELD_TYPES} />
          </Form.Item>

          {fieldType === 'select' ? (
            <Form.Item
              name="options"
              label="可选值"
              rules={[{ required: true, message: '请至少填一个可选值' }]}
              extra="输入后回车添加"
            >
              <Select mode="tags" open={false} placeholder="如：紧急、普通" />
            </Form.Item>
          ) : null}

          <Space size="large" wrap>
            <Form.Item name="required" label="必填" valuePropName="checked">
              <Switch />
            </Form.Item>
            <Form.Item
              name="isUniqueKey"
              label="唯一键"
              valuePropName="checked"
              extra="导入判重依据，全库只能有一个"
            >
              <Switch />
            </Form.Item>
            <Form.Item name="showInList" label="列表显示" valuePropName="checked">
              <Switch />
            </Form.Item>
            <Form.Item name="searchable" label="可搜索" valuePropName="checked">
              <Switch />
            </Form.Item>
            <Form.Item name="enabled" label="启用" valuePropName="checked">
              <Switch />
            </Form.Item>
          </Space>

          <Form.Item name="sort" label="排序" extra="数字越小越靠前">
            <InputNumber min={0} max={9999} />
          </Form.Item>
        </Form>
      </Modal>
    </Card>
  );
}
