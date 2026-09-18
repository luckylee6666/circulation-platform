import { useMemo, useState } from 'react';
import {
  App as AntApp,
  Button,
  Card,
  Drawer,
  Form,
  Input,
  InputNumber,
  Select,
  Space,
  Switch,
  Table,
  Tag,
  Typography,
} from 'antd';
import type { TableColumnsType } from 'antd';
import { DownloadOutlined, ReloadOutlined, UploadOutlined } from '@ant-design/icons';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useNavigate } from 'react-router-dom';

import { describeError } from '@/shared/api/client';
import { fieldsApi, recordsApi } from '@/shared/api/endpoints';
import type { FieldDef, RecordData, RecordItem } from '@/shared/api/types';
import { useAuth } from '@/shared/auth/AuthProvider';

import StartFlowModal from '../flows/StartFlowModal';

export default function RecordsPage() {
  const { message, modal } = AntApp.useApp();
  const { can } = useAuth();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const [form] = Form.useForm<RecordData>();

  const [keyword, setKeyword] = useState('');
  const [search, setSearch] = useState('');
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(20);
  const [editing, setEditing] = useState<RecordItem | null>(null);
  const [flowRecord, setFlowRecord] = useState<RecordItem | null>(null);
  const [saving, setSaving] = useState(false);

  const fields = useQuery({ queryKey: ['fields'], queryFn: fieldsApi.list });

  const records = useQuery({
    queryKey: ['records', search, page, pageSize],
    queryFn: () => recordsApi.list(search, page, pageSize),
  });

  const listFields = useMemo(
    () => (fields.data ?? []).filter((field) => field.enabled && field.showInList),
    [fields.data],
  );
  const allFields = useMemo(
    () => (fields.data ?? []).filter((field) => field.enabled),
    [fields.data],
  );

  const refresh = () => queryClient.invalidateQueries({ queryKey: ['records'] });

  const submitEdit = async () => {
    if (!editing) return;
    const values = await form.validateFields();
    setSaving(true);
    try {
      await recordsApi.update(editing.id, values);
      message.success('已保存');
      setEditing(null);
      await refresh();
    } catch (error) {
      message.error(describeError(error));
    } finally {
      setSaving(false);
    }
  };

  const remove = (record: RecordItem) => {
    modal.confirm({
      title: '删除这条记录？',
      content: `编号：${record.extKey}`,
      okText: '删除',
      okButtonProps: { danger: true },
      cancelText: '取消',
      onOk: async () => {
        try {
          await recordsApi.remove(record.id);
          message.success('已删除');
          await refresh();
        } catch (error) {
          message.error(describeError(error));
        }
      },
    });
  };

  const columns: TableColumnsType<RecordItem> = [
    ...listFields.map((field) => ({
      title: field.label,
      key: field.code,
      ellipsis: true,
      render: (_: unknown, record: RecordItem) => renderCell(record.data[field.code]),
    })),
    {
      title: '更新时间',
      dataIndex: 'updatedAt',
      key: 'updatedAt',
      width: 170,
    },
    {
      title: '操作',
      key: 'actions',
      width: 200,
      fixed: 'right' as const,
      render: (_: unknown, record: RecordItem) => (
        <Space size={4}>
          <Button
            type="link"
            size="small"
            disabled={!can('data:edit')}
            onClick={() => setEditing(record)}
          >
            编辑
          </Button>
          <Button
            type="link"
            size="small"
            disabled={!can('flow:create')}
            onClick={() => setFlowRecord(record)}
          >
            发起流转
          </Button>
          <Button
            type="link"
            size="small"
            danger
            disabled={!can('data:delete')}
            onClick={() => remove(record)}
          >
            删除
          </Button>
        </Space>
      ),
    },
  ];

  return (
    <Card
      title="数据管理"
      extra={
        <Space>
          <Input.Search
            allowClear
            placeholder="搜索数据内容"
            value={keyword}
            style={{ width: 240 }}
            onChange={(event) => setKeyword(event.target.value)}
            onSearch={(value) => {
              setSearch(value.trim());
              setPage(1);
            }}
          />
          <Button icon={<ReloadOutlined />} onClick={refresh}>
            刷新
          </Button>
          {can('data:export') ? (
            <Button
              icon={<DownloadOutlined />}
              onClick={() => {
                window.location.href = recordsApi.exportUrl(search);
              }}
            >
              导出 Excel
            </Button>
          ) : null}
          {can('data:import') ? (
            <Button type="primary" icon={<UploadOutlined />} onClick={() => navigate('/data/import')}>
              导入数据
            </Button>
          ) : null}
        </Space>
      }
    >
      {listFields.length === 0 ? (
        <Typography.Text type="secondary">
          还没有配置字段定义。请先到「系统管理 → 字段定义」里定义数据有哪些字段，再导入数据。
        </Typography.Text>
      ) : (
        <Table<RecordItem>
          rowKey="id"
          size="medium"
          loading={records.isLoading}
          columns={columns}
          dataSource={records.data?.items ?? []}
          scroll={{ x: 'max-content' }}
          pagination={{
            current: page,
            pageSize,
            total: records.data?.total ?? 0,
            showSizeChanger: true,
            showTotal: (total) => `共 ${total} 条`,
            onChange: (nextPage, nextSize) => {
              setPage(nextPage);
              setPageSize(nextSize);
            },
          }}
        />
      )}

      <Drawer
        title={editing ? `编辑记录：${editing.extKey}` : ''}
        size="large"
        open={editing !== null}
        onClose={() => setEditing(null)}
        destroyOnHidden
        footer={
          <Space>
            <Button onClick={() => setEditing(null)}>取消</Button>
            <Button type="primary" loading={saving} onClick={submitEdit}>
              保存
            </Button>
          </Space>
        }
      >
        {editing ? (
          <Form<RecordData>
            form={form}
            layout="vertical"
            preserve={false}
            initialValues={editing.data}
          >
            <Form.Item label="编号">
              <Typography.Text code>{editing.extKey}</Typography.Text>
            </Form.Item>

            {allFields.map((field) => (
              <Form.Item
                key={field.code}
                name={field.code}
                label={field.label}
                valuePropName={field.fieldType === 'bool' ? 'checked' : 'value'}
                rules={
                  field.required && field.fieldType !== 'bool'
                    ? [{ required: true, message: `请填写${field.label}` }]
                    : undefined
                }
              >
                {renderEditor(field)}
              </Form.Item>
            ))}
          </Form>
        ) : null}
      </Drawer>

      <StartFlowModal
        open={flowRecord !== null}
        record={flowRecord}
        onClose={() => setFlowRecord(null)}
        onCreated={(id) => navigate(`/flows/${id}`)}
      />
    </Card>
  );
}

function renderEditor(field: FieldDef) {
  switch (field.fieldType) {
    case 'number':
      return <InputNumber style={{ width: '100%' }} />;
    case 'bool':
      return <Switch />;
    case 'select':
      return <Select allowClear options={field.options.map((option) => ({ value: option, label: option }))} />;
    case 'date':
      return <Input placeholder="如 2026-01-05 或 2026-01-05 08:30:00" />;
    default:
      return <Input />;
  }
}

function renderCell(value: unknown) {
  if (value === null || value === undefined || value === '') {
    return <Typography.Text type="secondary">—</Typography.Text>;
  }
  if (typeof value === 'boolean') {
    return <Tag color={value ? 'green' : 'default'}>{value ? '是' : '否'}</Tag>;
  }
  return String(value);
}
