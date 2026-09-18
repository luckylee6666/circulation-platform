import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { Button, Card, Space, Table, Tabs, Tag, Typography } from 'antd';
import type { TableColumnsType } from 'antd';
import { PlusOutlined } from '@ant-design/icons';
import { useQuery } from '@tanstack/react-query';

import { flowsApi } from '@/shared/api/endpoints';
import type { FlowTask, InstanceItem } from '@/shared/api/types';
import { useAuth } from '@/shared/auth/AuthProvider';

import StartFlowModal from './StartFlowModal';
import { instanceStatus, stepKindLabel } from './labels';

const { Text } = Typography;

export default function FlowCenterPage() {
  const navigate = useNavigate();
  const { can } = useAuth();
  const [tab, setTab] = useState('todo');
  const [startOpen, setStartOpen] = useState(false);

  const todo = useQuery({
    queryKey: ['flow-tasks', 'todo'],
    queryFn: () => flowsApi.tasks(false),
    enabled: tab === 'todo',
  });
  const done = useQuery({
    queryKey: ['flow-tasks', 'done'],
    queryFn: () => flowsApi.tasks(true),
    enabled: tab === 'done',
  });
  const created = useQuery({
    queryKey: ['flow-instances', 'created'],
    queryFn: () => flowsApi.instances('created'),
    enabled: tab === 'created',
  });
  const involved = useQuery({
    queryKey: ['flow-instances', 'involved'],
    queryFn: () => flowsApi.instances('involved'),
    enabled: tab === 'involved',
  });

  const taskColumns: TableColumnsType<FlowTask> = [
    {
      title: '事项',
      dataIndex: 'title',
      render: (title: string, task) => (
        <a onClick={() => navigate(`/flows/${task.instanceId}`)}>{title}</a>
      ),
    },
    {
      title: '当前环节',
      dataIndex: 'stepName',
      width: 140,
      render: (name: string, task) => (
        <Space size={4}>
          <Tag>{stepKindLabel(task.stepType)}</Tag>
          {name}
        </Space>
      ),
    },
    { title: '发起人', dataIndex: 'initiatorName', width: 120 },
    { title: '到达时间', dataIndex: 'createdAt', width: 170 },
    {
      title: '操作',
      width: 100,
      render: (_, task) =>
        task.status === 0 ? (
          <Button type="link" size="small" onClick={() => navigate(`/flows/${task.instanceId}`)}>
            去处理
          </Button>
        ) : null,
    },
  ];

  const instanceColumns: TableColumnsType<InstanceItem> = [
    {
      title: '事项',
      dataIndex: 'title',
      render: (title: string, item) => (
        <a onClick={() => navigate(`/flows/${item.id}`)}>{title}</a>
      ),
    },
    { title: '编号', dataIndex: 'code', width: 170, render: (code: string) => <Text code>{code}</Text> },
    { title: '流程', dataIndex: 'defName', width: 140 },
    {
      title: '当前环节',
      dataIndex: 'currentStepName',
      width: 160,
      render: (name: string, item) =>
        item.status === 1 ? name : <Text type="secondary">—</Text>,
    },
    {
      title: '状态',
      dataIndex: 'status',
      width: 100,
      render: (status: number) => {
        const info = instanceStatus(status);
        return <Tag color={info.color}>{info.label}</Tag>;
      },
    },
    { title: '发起人', dataIndex: 'initiatorName', width: 110 },
    {
      title: '待办',
      dataIndex: 'pendingCount',
      width: 80,
      render: (count: number) => (count > 0 ? <Tag color="processing">{count}</Tag> : '—'),
    },
    { title: '更新时间', dataIndex: 'updatedAt', width: 170 },
    {
      title: '操作',
      width: 90,
      render: (_, item) => (
        <Button type="link" size="small" onClick={() => navigate(`/flows/${item.id}`)}>
          查看
        </Button>
      ),
    },
  ];

  const items = [
    {
      key: 'todo',
      label: '我的待办',
      children: (
        <Table
          rowKey="id"
          size="medium"
          loading={todo.isLoading}
          columns={taskColumns}
          dataSource={todo.data ?? []}
          pagination={{ pageSize: 20, hideOnSinglePage: true }}
          locale={{ emptyText: '没有待办事项' }}
        />
      ),
    },
    {
      key: 'done',
      label: '我的已办',
      children: (
        <Table
          rowKey="id"
          size="medium"
          loading={done.isLoading}
          columns={taskColumns}
          dataSource={done.data ?? []}
          pagination={{ pageSize: 20, hideOnSinglePage: true }}
          locale={{ emptyText: '还没有处理过的事项' }}
        />
      ),
    },
    {
      key: 'created',
      label: '我发起的',
      children: (
        <Table
          rowKey="id"
          size="medium"
          loading={created.isLoading}
          columns={instanceColumns}
          dataSource={created.data ?? []}
          pagination={{ pageSize: 20, hideOnSinglePage: true }}
          locale={{ emptyText: '还没有发起过流转' }}
        />
      ),
    },
    {
      key: 'involved',
      label: '我参与的',
      children: (
        <Table
          rowKey="id"
          size="medium"
          loading={involved.isLoading}
          columns={instanceColumns}
          dataSource={involved.data ?? []}
          pagination={{ pageSize: 20, hideOnSinglePage: true }}
          locale={{ emptyText: '还没有参与过流转' }}
        />
      ),
    },
  ];

  return (
    <Card
      title="流转中心"
      extra={
        can('flow:create') ? (
          <Button type="primary" icon={<PlusOutlined />} onClick={() => setStartOpen(true)}>
            发起流转
          </Button>
        ) : null
      }
    >
      <Tabs activeKey={tab} onChange={setTab} items={items} />

      <StartFlowModal
        open={startOpen}
        onClose={() => setStartOpen(false)}
        onCreated={(id) => navigate(`/flows/${id}`)}
      />
    </Card>
  );
}
