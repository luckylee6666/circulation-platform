import { useState } from 'react';
import { useNavigate, useParams } from 'react-router-dom';
import {
  App as AntApp,
  Button,
  Card,
  Descriptions,
  Input,
  Modal,
  Space,
  Spin,
  Table,
  Tag,
  Timeline,
  Typography,
} from 'antd';
import type { TableColumnsType } from 'antd';
import { ArrowLeftOutlined, StopOutlined } from '@ant-design/icons';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import { describeError } from '@/shared/api/client';
import { flowsApi } from '@/shared/api/endpoints';
import type { FlowTask } from '@/shared/api/types';
import { useAuth } from '@/shared/auth/AuthProvider';

import TaskActionModal from './TaskActionModal';
import { instanceStatus, stepKindLabel } from './labels';

const { Text, Title } = Typography;

export default function InstanceDetailPage() {
  const { id } = useParams();
  const instanceId = Number(id);
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const { message } = AntApp.useApp();
  const { can } = useAuth();

  const [action, setAction] = useState<'complete' | 'pass' | 'reject' | null>(null);
  const [terminateOpen, setTerminateOpen] = useState(false);
  const [reason, setReason] = useState('');

  const detail = useQuery({
    queryKey: ['flow-instance', instanceId],
    queryFn: () => flowsApi.instance(instanceId),
    enabled: Number.isFinite(instanceId),
  });

  const refresh = () => {
    queryClient.invalidateQueries({ queryKey: ['flow-instance', instanceId] });
    queryClient.invalidateQueries({ queryKey: ['flow-tasks'] });
    queryClient.invalidateQueries({ queryKey: ['flow-instances'] });
  };

  const terminate = useMutation({
    mutationFn: () => flowsApi.terminate(instanceId, reason.trim()),
    onSuccess: () => {
      message.success('流程已终止');
      setTerminateOpen(false);
      setReason('');
      refresh();
    },
    onError: (error) => message.error(describeError(error)),
  });

  if (detail.isLoading) {
    return (
      <Card>
        <Spin description="加载中…" />
      </Card>
    );
  }

  if (!detail.data) {
    return (
      <Card>
        <Text type="secondary">流程不存在或已被删除</Text>
        <div style={{ marginTop: 12 }}>
          <Button onClick={() => navigate('/flows')}>返回流转中心</Button>
        </div>
      </Card>
    );
  }

  const data = detail.data;
  const status = instanceStatus(data.status);
  const active = data.status === 1;
  const myTask = data.myPendingTaskId;
  const stepType = data.currentStepType;

  const taskColumns: TableColumnsType<FlowTask> = [
    {
      title: '环节',
      width: 150,
      render: (_, task) => (
        <Space size={4}>
          <Tag>{stepKindLabel(task.stepType)}</Tag>
          {task.stepName}
        </Space>
      ),
    },
    { title: '处理人', dataIndex: 'assigneeName', width: 120 },
    {
      title: '状态',
      dataIndex: 'status',
      width: 110,
      render: (value: number, task) => {
        if (value === 0) return <Tag color="processing">待处理</Tag>;
        if (value === 2) return <Text type="secondary">已作废</Text>;
        return <Tag color="success">{task.action || '已完成'}</Tag>;
      },
    },
    {
      title: '说明',
      dataIndex: 'comment',
      render: (comment: string) => comment || <Text type="secondary">—</Text>,
    },
    { title: '到达', dataIndex: 'createdAt', width: 170 },
    {
      title: '处理于',
      dataIndex: 'doneAt',
      width: 170,
      render: (value: string | null) => value ?? <Text type="secondary">—</Text>,
    },
  ];

  const formEntries = Object.entries(data.form).filter(
    ([, value]) => value !== null && value !== undefined && String(value).trim() !== '',
  );

  return (
    <Space orientation="vertical" size="middle" style={{ width: '100%' }}>
      <Card>
        <Space orientation="vertical" size="middle" style={{ width: '100%' }}>
          <Space>
            <Button icon={<ArrowLeftOutlined />} onClick={() => navigate('/flows')}>
              返回
            </Button>
            <Title level={4} style={{ margin: 0 }}>
              {data.title}
            </Title>
            <Tag color={status.color}>{status.label}</Tag>
          </Space>

          <Descriptions
            size="small"
            column={{ xs: 1, sm: 2, md: 3 }}
            items={[
              { key: 'code', label: '编号', children: <Text code>{data.code}</Text> },
              { key: 'def', label: '流程', children: data.defName },
              { key: 'initiator', label: '发起人', children: data.initiatorName },
              { key: 'created', label: '发起时间', children: data.createdAt },
              {
                key: 'step',
                label: '当前环节',
                children: active ? (
                  <Space size={4}>
                    <Tag>{stepKindLabel(stepType)}</Tag>
                    {data.currentStepName}
                  </Space>
                ) : (
                  <Text type="secondary">—</Text>
                ),
              },
              {
                key: 'finished',
                label: '结束时间',
                children: data.finishedAt ?? <Text type="secondary">—</Text>,
              },
            ]}
          />

          {active ? (
            <Space wrap>
              {myTask ? (
                <>
                  {stepType === 'dispatch' ? (
                    <Button type="primary" onClick={() => setAction('complete')}>
                      分派给承办人
                    </Button>
                  ) : null}
                  {stepType === 'handle' ? (
                    <Button type="primary" onClick={() => setAction('complete')}>
                      提交承办结果
                    </Button>
                  ) : null}
                  {stepType === 'confirm' ? (
                    <>
                      <Button type="primary" onClick={() => setAction('pass')}>
                        确认通过
                      </Button>
                      <Button danger onClick={() => setAction('reject')}>
                        打回重做
                      </Button>
                    </>
                  ) : null}
                </>
              ) : (
                <Text type="secondary">当前环节由其他人处理，你暂时不需要操作</Text>
              )}

              {can('flow:terminate') ? (
                <Button icon={<StopOutlined />} onClick={() => setTerminateOpen(true)}>
                  终止流程
                </Button>
              ) : null}
            </Space>
          ) : null}
        </Space>
      </Card>

      {formEntries.length > 0 ? (
        <Card title="事项信息" size="small">
          <Descriptions
            size="small"
            column={{ xs: 1, sm: 2, md: 3 }}
            items={formEntries.map(([key, value]) => ({
              key,
              label: key,
              children: String(value),
            }))}
          />
        </Card>
      ) : null}

      <Card title="处理记录" size="small">
        <Timeline
          items={data.logs.map((log) => ({
            color: log.action === '打回' ? 'red' : log.action === '发起' ? 'blue' : 'green',
            content: (
              <Space orientation="vertical" size={2}>
                <Space size={8}>
                  <Text strong>{log.action}</Text>
                  <Text type="secondary">{log.actorName}</Text>
                  <Text type="secondary">{log.createdAt}</Text>
                </Space>
                {log.detail ? <Text>{log.detail}</Text> : null}
                {log.fromStep && log.toStep && log.toStep !== log.fromStep ? (
                  <Text type="secondary">
                    {log.fromStep} → {log.toStep}
                  </Text>
                ) : null}
              </Space>
            ),
          }))}
        />
      </Card>

      <Card title="环节任务" size="small">
        <Table
          rowKey="id"
          size="small"
          columns={taskColumns}
          dataSource={data.tasks}
          pagination={false}
        />
      </Card>

      <TaskActionModal
        open={action !== null}
        taskId={myTask}
        stepType={stepType}
        action={action ?? 'complete'}
        stepName={data.currentStepName}
        onClose={() => setAction(null)}
        onDone={refresh}
      />

      <Modal
        open={terminateOpen}
        title="终止流程"
        okText="确认终止"
        okButtonProps={{ danger: true, loading: terminate.isPending }}
        cancelText="取消"
        onOk={() => terminate.mutate()}
        onCancel={() => setTerminateOpen(false)}
        destroyOnHidden
      >
        <Text type="secondary">终止后所有待办会作废，流程不可恢复。</Text>
        <Input.TextArea
          rows={3}
          style={{ marginTop: 12 }}
          placeholder="终止原因（可选）"
          value={reason}
          onChange={(event) => setReason(event.target.value)}
          maxLength={200}
        />
      </Modal>
    </Space>
  );
}
