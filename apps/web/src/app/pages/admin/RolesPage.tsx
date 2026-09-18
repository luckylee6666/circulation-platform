import { useMemo, useState } from 'react';
import {
  App as AntApp,
  Button,
  Card,
  Checkbox,
  Divider,
  Drawer,
  Form,
  Input,
  Space,
  Table,
  Tag,
  Typography,
} from 'antd';
import type { TableColumnsType } from 'antd';
import { PlusOutlined, ReloadOutlined } from '@ant-design/icons';
import { useQuery, useQueryClient } from '@tanstack/react-query';

import { describeError } from '@/shared/api/client';
import { rolesApi } from '@/shared/api/endpoints';
import type { Permission, Role } from '@/shared/api/types';

interface RoleFormValues {
  code: string;
  name: string;
  description: string;
  permissions: string[];
}

export default function RolesPage() {
  const { message, modal } = AntApp.useApp();
  const queryClient = useQueryClient();
  const [form] = Form.useForm<RoleFormValues>();

  const [editing, setEditing] = useState<Role | null>(null);
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [saving, setSaving] = useState(false);

  const roles = useQuery({ queryKey: ['roles'], queryFn: rolesApi.list });
  const permissions = useQuery({ queryKey: ['permissions'], queryFn: rolesApi.permissions });

  const grouped = useMemo(() => {
    const map = new Map<string, Permission[]>();
    for (const permission of permissions.data ?? []) {
      const list = map.get(permission.category) ?? [];
      list.push(permission);
      map.set(permission.category, list);
    }
    return [...map.entries()];
  }, [permissions.data]);

  const refresh = () => {
    queryClient.invalidateQueries({ queryKey: ['roles'] });
    queryClient.invalidateQueries({ queryKey: ['permissions'] });
  };

  const openCreate = () => {
    setEditing(null);
    setDrawerOpen(true);
  };

  const openEdit = (role: Role) => {
    setEditing(role);
    setDrawerOpen(true);
  };

  const submit = async () => {
    const values = await form.validateFields();
    setSaving(true);
    try {
      const payload = {
        code: values.code.trim(),
        name: values.name.trim(),
        description: values.description ?? '',
        permissions: values.permissions ?? [],
      };

      if (editing) {
        await rolesApi.update(editing.id, payload);
        message.success('角色已更新');
      } else {
        await rolesApi.create(payload);
        message.success('角色已创建');
      }

      setDrawerOpen(false);
      setEditing(null);
      await refresh();
    } catch (error) {
      message.error(describeError(error));
    } finally {
      setSaving(false);
    }
  };

  const remove = (role: Role) => {
    modal.confirm({
      title: `删除角色「${role.name}」？`,
      content:
        role.userCount > 0
          ? `还有 ${role.userCount} 个用户属于该角色，需要先调整他们的角色。`
          : '删除后无法恢复。',
      okText: '删除',
      okButtonProps: { danger: true, disabled: role.userCount > 0 },
      cancelText: '取消',
      onOk: async () => {
        try {
          await rolesApi.remove(role.id);
          message.success('已删除');
          await refresh();
        } catch (error) {
          message.error(describeError(error));
        }
      },
    });
  };

  const columns: TableColumnsType<Role> = [
    {
      title: '角色',
      dataIndex: 'name',
      key: 'name',
      render: (_, record) => (
        <Space orientation="vertical" size={0}>
          <Space size={6}>
            <Typography.Text strong>{record.name}</Typography.Text>
            {record.isSystem ? <Tag variant="filled">内置</Tag> : null}
          </Space>
          <Typography.Text type="secondary" style={{ fontSize: 12 }}>
            {record.code}
          </Typography.Text>
        </Space>
      ),
    },
    {
      title: '说明',
      dataIndex: 'description',
      key: 'description',
      render: (value: string) => value || '—',
    },
    {
      title: '权限',
      dataIndex: 'permissions',
      key: 'permissions',
      width: 110,
      render: (value: string[]) => `${value.length} 项`,
    },
    {
      title: '用户数',
      dataIndex: 'userCount',
      key: 'userCount',
      width: 100,
    },
    {
      title: '操作',
      key: 'actions',
      width: 160,
      render: (_, record) => (
        <Space size={4}>
          <Button type="link" size="small" onClick={() => openEdit(record)}>
            配置权限
          </Button>
          <Button
            type="link"
            size="small"
            danger
            disabled={record.isSystem}
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
      title="角色权限"
      extra={
        <Space>
          <Button icon={<ReloadOutlined />} onClick={refresh}>
            刷新
          </Button>
          <Button type="primary" icon={<PlusOutlined />} onClick={openCreate}>
            新建角色
          </Button>
        </Space>
      }
    >
      <Typography.Paragraph type="secondary">
        角色决定用户能做什么。内置角色的标识不可修改，但权限可以按实际情况调整；
        系统管理员角色始终拥有全部权限。
      </Typography.Paragraph>

      <Table
        rowKey="id"
        size="medium"
        loading={roles.isLoading}
        columns={columns}
        dataSource={roles.data ?? []}
        pagination={false}
      />

      <Drawer
        title={editing ? `配置角色：${editing.name}` : '新建角色'}
        size="large"
        open={drawerOpen}
        onClose={() => {
          setDrawerOpen(false);
          setEditing(null);
        }}
        destroyOnHidden
        footer={
          <Space>
            <Button
              onClick={() => {
                setDrawerOpen(false);
                setEditing(null);
              }}
            >
              取消
            </Button>
            <Button type="primary" loading={saving} onClick={submit}>
              保存
            </Button>
          </Space>
        }
      >
        <Form<RoleFormValues>
          form={form}
          layout="vertical"
          preserve={false}
          initialValues={
            editing
              ? {
                  code: editing.code,
                  name: editing.name,
                  description: editing.description,
                  permissions: editing.permissions,
                }
              : { permissions: [] }
          }
        >
          <Form.Item
            name="code"
            label="角色标识"
            rules={[{ required: true, message: '请输入角色标识' }]}
            extra={editing?.isSystem ? '内置角色标识不可修改' : '英文小写，如 handler'}
          >
            <Input disabled={Boolean(editing?.isSystem)} />
          </Form.Item>

          <Form.Item
            name="name"
            label="角色名称"
            rules={[{ required: true, message: '请输入角色名称' }]}
          >
            <Input />
          </Form.Item>

          <Form.Item name="description" label="说明">
            <Input.TextArea rows={2} />
          </Form.Item>

          <Divider titlePlacement="start">权限</Divider>

          <Form.Item name="permissions" noStyle>
            <Checkbox.Group style={{ width: '100%' }}>
              {grouped.map(([category, items]) => (
                <div key={category} className="role-permission-group">
                  <Typography.Text strong className="role-permission-category">
                    {category}
                  </Typography.Text>
                  <Space orientation="vertical" size={6}>
                    {items.map((item) => (
                      <Checkbox key={item.code} value={item.code}>
                        {item.name}
                        <Typography.Text type="secondary" style={{ fontSize: 12, marginLeft: 6 }}>
                          {item.code}
                        </Typography.Text>
                      </Checkbox>
                    ))}
                  </Space>
                </div>
              ))}
            </Checkbox.Group>
          </Form.Item>
        </Form>
      </Drawer>
    </Card>
  );
}
