import { useMemo, useState } from 'react';
import {
  App as AntApp,
  Button,
  Card,
  Form,
  Input,
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
import { rolesApi, usersApi } from '@/shared/api/endpoints';
import type { User } from '@/shared/api/types';
import { useAuth } from '@/shared/auth/AuthProvider';

interface UserFormValues {
  username: string;
  displayName: string;
  password?: string;
  phone?: string;
  dept?: string;
  email?: string;
  roleIds: number[];
}

export default function UsersPage() {
  const { message, modal } = AntApp.useApp();
  const { user: currentUser } = useAuth();
  const queryClient = useQueryClient();
  const [form] = Form.useForm<UserFormValues>();

  const [keyword, setKeyword] = useState('');
  const [search, setSearch] = useState('');
  const [editing, setEditing] = useState<User | null>(null);
  const [formOpen, setFormOpen] = useState(false);
  const [saving, setSaving] = useState(false);

  const users = useQuery({
    queryKey: ['users', search],
    queryFn: () => usersApi.list(search),
  });

  const roles = useQuery({ queryKey: ['roles'], queryFn: rolesApi.list });

  const roleOptions = useMemo(
    () => (roles.data ?? []).map((role) => ({ label: `${role.name}（${role.code}）`, value: role.id })),
    [roles.data],
  );

  const refresh = () => queryClient.invalidateQueries({ queryKey: ['users'] });

  const openCreate = () => {
    setEditing(null);
    setFormOpen(true);
  };

  const openEdit = (record: User) => {
    setEditing(record);
    setFormOpen(true);
  };

  const submit = async () => {
    const values = await form.validateFields();
    setSaving(true);
    try {
      if (editing) {
        await usersApi.update(editing.id, {
          username: editing.username,
          displayName: values.displayName,
          phone: values.phone ?? '',
          dept: values.dept ?? '',
          email: values.email ?? '',
          roleIds: values.roleIds ?? [],
        });
        message.success('已保存');
      } else {
        const created = await usersApi.create({
          username: values.username.trim(),
          displayName: values.displayName,
          password: values.password ?? '',
          phone: values.phone ?? '',
          dept: values.dept ?? '',
          email: values.email ?? '',
          roleIds: values.roleIds ?? [],
        });
        modal.success({
          title: '用户已创建',
          content: (
            <div>
              <p>
                初始密码：<Typography.Text code>{created.initialPassword}</Typography.Text>
              </p>
              <p style={{ marginBottom: 0 }}>
                请把登录名和这个初始密码告知本人，对方首次登录时必须修改密码。
              </p>
            </div>
          ),
        });
      }

      setFormOpen(false);
      setEditing(null);
      await refresh();
    } catch (error) {
      message.error(describeError(error));
    } finally {
      setSaving(false);
    }
  };

  const toggleStatus = async (record: User) => {
    const next = record.status === 1 ? 0 : 1;
    try {
      await usersApi.setStatus(record.id, next);
      message.success(next === 1 ? '已启用' : '已停用，该账号的登录已全部失效');
      await refresh();
    } catch (error) {
      message.error(describeError(error));
    }
  };

  const resetPassword = (record: User) => {
    modal.confirm({
      title: `重置「${record.displayName}」的密码？`,
      content: '重置后会生成一个新的初始密码，该账号当前的登录会全部失效。',
      okText: '确认重置',
      cancelText: '取消',
      onOk: async () => {
        try {
          const result = await usersApi.resetPassword(record.id, '');
          modal.success({
            title: '密码已重置',
            content: (
              <p style={{ marginBottom: 0 }}>
                新的初始密码：<Typography.Text code>{result.password}</Typography.Text>
                <br />
                对方下次登录时必须修改密码。
              </p>
            ),
          });
          await refresh();
        } catch (error) {
          message.error(describeError(error));
        }
      },
    });
  };

  const remove = (record: User) => {
    modal.confirm({
      title: `删除用户「${record.displayName}」？`,
      content: '删除后无法恢复，该账号的所有登录会立即失效。',
      okText: '删除',
      okButtonProps: { danger: true },
      cancelText: '取消',
      onOk: async () => {
        try {
          await usersApi.remove(record.id);
          message.success('已删除');
          await refresh();
        } catch (error) {
          message.error(describeError(error));
        }
      },
    });
  };

  const columns: TableColumnsType<User> = [
    {
      title: '姓名',
      dataIndex: 'displayName',
      key: 'displayName',
      render: (_, record) => (
        <Space orientation="vertical" size={0}>
          <Typography.Text strong>{record.displayName}</Typography.Text>
          <Typography.Text type="secondary" style={{ fontSize: 12 }}>
            {record.username}
          </Typography.Text>
        </Space>
      ),
    },
    { title: '部门', dataIndex: 'dept', key: 'dept', width: 160, render: (v: string) => v || '—' },
    {
      title: '角色',
      dataIndex: 'roles',
      key: 'roles',
      render: (_, record) =>
        record.roles.length === 0 ? (
          <Typography.Text type="secondary">未分配</Typography.Text>
        ) : (
          <Space size={4} wrap>
            {record.roles.map((role) => (
              <Tag key={role.id} color="blue">
                {role.name}
              </Tag>
            ))}
          </Space>
        ),
    },
    {
      title: '状态',
      dataIndex: 'status',
      key: 'status',
      width: 110,
      render: (_, record) => {
        const isSelf = record.id === currentUser?.id;
        return (
          <Space size={6}>
            <Switch
              size="small"
              checked={record.status === 1}
              disabled={isSelf}
              onChange={() => toggleStatus(record)}
            />
            <Typography.Text type="secondary" style={{ fontSize: 12 }}>
              {record.status === 1 ? '启用' : '停用'}
            </Typography.Text>
          </Space>
        );
      },
    },
    {
      title: '最后登录',
      dataIndex: 'lastLoginAt',
      key: 'lastLoginAt',
      width: 170,
      render: (value: string | null) =>
        value ?? <Typography.Text type="secondary">从未登录</Typography.Text>,
    },
    {
      title: '操作',
      key: 'actions',
      width: 210,
      render: (_, record) => {
        const isSelf = record.id === currentUser?.id;
        return (
          <Space size={4} wrap>
            <Button type="link" size="small" onClick={() => openEdit(record)}>
              编辑
            </Button>
            <Button type="link" size="small" onClick={() => resetPassword(record)}>
              重置密码
            </Button>
            <Button
              type="link"
              size="small"
              danger={!isSelf}
              disabled={isSelf}
              onClick={() => remove(record)}
            >
              删除
            </Button>
          </Space>
        );
      },
    },
  ];

  return (
    <Card
      title="用户管理"
      extra={
        <Space>
          <Input.Search
            allowClear
            placeholder="搜索姓名 / 登录名 / 部门"
            value={keyword}
            onChange={(event) => setKeyword(event.target.value)}
            onSearch={(value) => setSearch(value.trim())}
            style={{ width: 240 }}
          />
          <Button icon={<ReloadOutlined />} onClick={refresh}>
            刷新
          </Button>
          <Button type="primary" icon={<PlusOutlined />} onClick={openCreate}>
            新建用户
          </Button>
        </Space>
      }
    >
      <Table
        rowKey="id"
        size="medium"
        loading={users.isLoading}
        columns={columns}
        dataSource={users.data ?? []}
        pagination={{ pageSize: 20, showSizeChanger: false, hideOnSinglePage: true }}
      />

      <Modal
        title={editing ? `编辑用户：${editing.displayName}` : '新建用户'}
        open={formOpen}
        onCancel={() => {
          setFormOpen(false);
          setEditing(null);
        }}
        onOk={submit}
        confirmLoading={saving}
        okText="保存"
        cancelText="取消"
        destroyOnHidden
      >
        <Form<UserFormValues>
          form={form}
          layout="vertical"
          preserve={false}
          initialValues={
            editing
              ? {
                  username: editing.username,
                  displayName: editing.displayName,
                  phone: editing.phone,
                  dept: editing.dept,
                  email: editing.email,
                  roleIds: editing.roles.map((role) => role.id),
                }
              : { roleIds: [] }
          }
        >
          <Form.Item
            name="username"
            label="登录名"
            rules={[{ required: true, message: '请输入登录名' }]}
            extra={editing ? '登录名创建后不可修改' : '建议使用姓名拼音，2-32 个字符'}
          >
            <Input disabled={Boolean(editing)} />
          </Form.Item>

          <Form.Item
            name="displayName"
            label="姓名"
            rules={[{ required: true, message: '请输入姓名' }]}
          >
            <Input />
          </Form.Item>

          {editing ? null : (
            <Form.Item name="password" label="初始密码" extra="留空则使用默认密码 123456">
              <Input placeholder="留空使用默认密码" />
            </Form.Item>
          )}

          <Form.Item name="dept" label="部门">
            <Input placeholder="如：业务一科" />
          </Form.Item>

          <Form.Item name="phone" label="手机号">
            <Input />
          </Form.Item>

          <Form.Item name="email" label="邮箱">
            <Input />
          </Form.Item>

          <Form.Item name="roleIds" label="角色" extra="角色决定了这个人能做什么">
            <Select mode="multiple" options={roleOptions} placeholder="选择角色" allowClear />
          </Form.Item>
        </Form>
      </Modal>
    </Card>
  );
}
