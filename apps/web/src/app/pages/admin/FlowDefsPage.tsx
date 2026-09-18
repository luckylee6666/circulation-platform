import { useEffect, useState } from 'react';
import {
  App as AntApp,
  Button,
  Card,
  Collapse,
  Divider,
  Empty,
  Input,
  Modal,
  Popconfirm,
  Radio,
  Select,
  Space,
  Switch,
  Table,
  Tag,
  Typography,
} from 'antd';
import type { TableColumnsType } from 'antd';
import {
  ArrowDownOutlined,
  ArrowUpOutlined,
  DeleteOutlined,
  PlusOutlined,
} from '@ant-design/icons';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import { describeError } from '@/shared/api/client';
import { flowsApi, rolesApi, usersApi } from '@/shared/api/endpoints';
import type {
  Assignee,
  ConditionOp,
  FlowConfig,
  FlowDef,
  RoutingRule,
  StepConfig,
  StepKind,
} from '@/shared/api/types';

import { STEP_KINDS, assigneeLabel, stepSummary } from '../flows/labels';

const { Text } = Typography;

const END = 'end';

interface Draft {
  id: number | null;
  code: string;
  name: string;
  description: string;
  enabled: boolean;
  isDefault: boolean;
  config: FlowConfig;
}

const CONDITION_OPS: { value: ConditionOp; label: string }[] = [
  { value: 'eq', label: '等于' },
  { value: 'ne', label: '不等于' },
  { value: 'gt', label: '大于' },
  { value: 'lt', label: '小于' },
  { value: 'contains', label: '包含' },
  { value: 'empty', label: '为空' },
  { value: 'notEmpty', label: '不为空' },
];

function defaultAssignee(mode: Assignee['mode']): Assignee {
  switch (mode) {
    case 'role':
      return { mode: 'role', roles: [] };
    case 'users':
      return { mode: 'users', userIds: [] };
    case 'assigned':
      return { mode: 'assigned' };
    default:
      return { mode: 'initiator' };
  }
}

function blankStep(index: number): StepConfig {
  return {
    key: `s${index + 1}`,
    name: `步骤${index + 1}`,
    kind: index === 0 ? 'start' : 'handle',
    assignee: index === 0 ? { mode: 'initiator' } : { mode: 'assigned' },
    next: null,
    onReject: null,
    completeRule: 'all',
    assignMode: 'manual',
  };
}

function blankRule(stepKey: string): RoutingRule {
  return { onStep: stepKey, when: { field: '', op: 'eq', value: '' }, goto: END };
}

function toDraft(def: FlowDef): Draft {
  return {
    id: def.id,
    code: def.code,
    name: def.name,
    description: def.description,
    enabled: def.enabled,
    isDefault: def.isDefault,
    config: {
      steps: def.config.steps.map((step) => ({ ...step })),
      rules: def.config.rules.map((rule) => ({ ...rule })),
    },
  };
}

function newDraft(): Draft {
  return {
    id: null,
    code: '',
    name: '',
    description: '',
    enabled: true,
    isDefault: false,
    config: {
      steps: [
        { ...blankStep(0), name: '发起' },
        { ...blankStep(1), kind: 'dispatch', name: '分派', assignee: { mode: 'role', roles: ['dispatcher'] } },
        { ...blankStep(2), kind: 'handle', name: '承办', assignee: { mode: 'assigned' } },
        { ...blankStep(3), kind: 'confirm', name: '确认', assignee: { mode: 'initiator' }, onReject: 's2' },
      ],
      rules: [],
    },
  };
}

export default function FlowDefsPage() {
  const { message } = AntApp.useApp();
  const queryClient = useQueryClient();
  const [draft, setDraft] = useState<Draft | null>(null);

  const defs = useQuery({ queryKey: ['flow-defs'], queryFn: flowsApi.defs });
  const roles = useQuery({ queryKey: ['roles'], queryFn: rolesApi.list });
  const users = useQuery({ queryKey: ['user-options'], queryFn: usersApi.options });

  const roleOptions = (roles.data ?? []).map((role) => ({
    value: role.code,
    label: `${role.name}（${role.code}）`,
  }));
  const userOptions = (users.data ?? []).map((user) => ({
    value: user.id,
    label: user.dept ? `${user.displayName}（${user.dept}）` : user.displayName,
  }));

  const save = useMutation({
    mutationFn: async (value: Draft): Promise<void> => {
      const payload = {
        code: value.code.trim(),
        name: value.name.trim(),
        description: value.description,
        config: value.config,
        enabled: value.enabled,
        isDefault: value.isDefault,
      };
      if (value.id === null) {
        await flowsApi.createDef(payload);
      } else {
        await flowsApi.updateDef(value.id, payload);
      }
    },
    onSuccess: () => {
      message.success('已保存');
      setDraft(null);
      queryClient.invalidateQueries({ queryKey: ['flow-defs'] });
    },
    onError: (error) => message.error(describeError(error)),
  });

  const remove = useMutation({
    mutationFn: (id: number) => flowsApi.removeDef(id),
    onSuccess: () => {
      message.success('已删除');
      queryClient.invalidateQueries({ queryKey: ['flow-defs'] });
    },
    onError: (error) => message.error(describeError(error)),
  });

  const columns: TableColumnsType<FlowDef> = [
    { title: '名称', dataIndex: 'name' },
    {
      title: '标识',
      dataIndex: 'code',
      width: 140,
      render: (code: string) => <Text code>{code}</Text>,
    },
    {
      title: '步骤',
      width: 260,
      render: (_, def) => (
        <Space size={4} wrap>
          {def.config.steps.map((step) => (
            <Tag key={step.key}>{step.name}</Tag>
          ))}
        </Space>
      ),
    },
    {
      title: '版本',
      dataIndex: 'version',
      width: 80,
      render: (version: number) => `v${version}`,
    },
    {
      title: '状态',
      width: 90,
      render: (_, def) => (
        <Space size={4}>
          {def.enabled ? <Tag color="success">启用</Tag> : <Tag>停用</Tag>}
          {def.isDefault ? <Tag color="blue">默认</Tag> : null}
        </Space>
      ),
    },
    {
      title: '操作',
      width: 140,
      render: (_, def) => (
        <Space size={0}>
          <Button type="link" size="small" onClick={() => setDraft(toDraft(def))}>
            编辑
          </Button>
          <Popconfirm
            title="删除这个流程？"
            description="已有流转在用的流程不能删除。"
            okText="删除"
            cancelText="取消"
            onConfirm={() => remove.mutate(def.id)}
          >
            <Button type="link" size="small" danger>
              删除
            </Button>
          </Popconfirm>
        </Space>
      ),
    },
  ];

  return (
    <Card
      title="流程配置"
      extra={
        <Button type="primary" icon={<PlusOutlined />} onClick={() => setDraft(newDraft())}>
          新建流程
        </Button>
      }
    >
      <Table
        rowKey="id"
        size="medium"
        loading={defs.isLoading}
        columns={columns}
        dataSource={defs.data ?? []}
        pagination={false}
      />

      {draft ? (
        <FlowDefDrawer
          draft={draft}
          onChange={setDraft}
          roleOptions={roleOptions}
          userOptions={userOptions}
          saving={save.isPending}
          onSave={() => save.mutate(draft)}
          onClose={() => setDraft(null)}
        />
      ) : null}
    </Card>
  );
}

interface DrawerProps {
  draft: Draft;
  onChange: (draft: Draft) => void;
  roleOptions: { value: string; label: string }[];
  userOptions: { value: number; label: string }[];
  saving: boolean;
  onSave: () => void;
  onClose: () => void;
}

function FlowDefDrawer({
  draft,
  onChange,
  roleOptions,
  userOptions,
  saving,
  onSave,
  onClose,
}: DrawerProps) {
  const [stepTab, setStepTab] = useState<string | null>(null);

  const patchConfig = (patch: Partial<FlowConfig>) =>
    onChange({ ...draft, config: { ...draft.config, ...patch } });

  const updateStep = (index: number, patch: Partial<StepConfig>) => {
    const steps = draft.config.steps.map((step, i) => (i === index ? { ...step, ...patch } : step));
    patchConfig({ steps });
  };

  const moveStep = (index: number, delta: number) => {
    const steps = [...draft.config.steps];
    const target = index + delta;
    if (target < 0 || target >= steps.length) return;
    const [moved] = steps.splice(index, 1);
    steps.splice(target, 0, moved);
    patchConfig({ steps });
  };

  const removeStep = (index: number) => {
    patchConfig({ steps: draft.config.steps.filter((_, i) => i !== index) });
  };

  const addStep = () => {
    const steps = [...draft.config.steps, blankStep(draft.config.steps.length)];
    patchConfig({ steps });
    setStepTab(steps[steps.length - 1].key);
  };

  const updateRule = (index: number, patch: Partial<RoutingRule>) => {
    patchConfig({
      rules: draft.config.rules.map((rule, i) => (i === index ? { ...rule, ...patch } : rule)),
    });
  };

  const stepTargetOptions = [
    ...draft.config.steps.map((step) => ({ value: step.key, label: `${step.name}（${step.key}）` })),
    { value: END, label: '结束流程' },
  ];

  useEffect(() => {
    if (stepTab === null && draft.config.steps.length > 0) {
      setStepTab(draft.config.steps[0].key);
    }
  }, [draft.config.steps, stepTab]);

  return (
    <Modal
      open
      width={900}
      title={draft.id === null ? '新建流程' : `编辑流程：${draft.name}`}
      okText="保存"
      cancelText="取消"
      confirmLoading={saving}
      onOk={onSave}
      onCancel={onClose}
      styles={{ body: { maxHeight: '70vh', overflowY: 'auto' } }}
    >
      <Space orientation="vertical" size="middle" style={{ width: '100%' }}>
        <Space wrap size="middle">
          <Space size={8}>
            <Text>名称</Text>
            <Input
              style={{ width: 200 }}
              value={draft.name}
              onChange={(event) => onChange({ ...draft, name: event.target.value })}
              placeholder="例如：设备报修流转"
            />
          </Space>
          <Space size={8}>
            <Text>标识</Text>
            <Input
              style={{ width: 150 }}
              value={draft.code}
              disabled={draft.id !== null}
              onChange={(event) => onChange({ ...draft, code: event.target.value })}
              placeholder="英文标识"
            />
          </Space>
          <Space size={8}>
            <Text>启用</Text>
            <Switch
              checked={draft.enabled}
              onChange={(checked) => onChange({ ...draft, enabled: checked })}
            />
          </Space>
          <Space size={8}>
            <Text>设为默认</Text>
            <Switch
              checked={draft.isDefault}
              onChange={(checked) => onChange({ ...draft, isDefault: checked })}
            />
          </Space>
        </Space>

        <Input
          value={draft.description}
          onChange={(event) => onChange({ ...draft, description: event.target.value })}
          placeholder="这个流程是干什么的（可选）"
          maxLength={200}
        />

        <Divider titlePlacement="start" style={{ margin: 0 }}>
          步骤
        </Divider>

        {draft.config.steps.length === 0 ? (
          <Empty description="还没有步骤" />
        ) : (
          <Collapse
            accordion
            activeKey={stepTab ?? undefined}
            onChange={(key) => setStepTab(Array.isArray(key) ? key[0] : key)}
            items={draft.config.steps.map((step, index) => ({
              key: step.key,
              label: (
                <Space size={8}>
                  <Tag>{index + 1}</Tag>
                  <Text strong>{step.name || '未命名步骤'}</Text>
                  <Text type="secondary">{stepSummary(step)}</Text>
                </Space>
              ),
              children: (
                <StepEditor
                  step={step}
                  index={index}
                  total={draft.config.steps.length}
                  roleOptions={roleOptions}
                  userOptions={userOptions}
                  stepTargetOptions={stepTargetOptions}
                  onChange={(patch) => updateStep(index, patch)}
                  onMove={(delta) => moveStep(index, delta)}
                  onRemove={() => removeStep(index)}
                />
              ),
            }))}
          />
        )}

        <Button type="dashed" icon={<PlusOutlined />} onClick={addStep} block>
          添加步骤
        </Button>

        <Divider titlePlacement="start" style={{ margin: 0 }}>
          条件判断
        </Divider>
        <Text type="secondary">
          满足条件时跳过默认的下一步。字段名填事项信息里的字段，例如记录里的「紧急程度」。
        </Text>

        {draft.config.rules.map((rule, index) => (
          <Space key={index} wrap align="center">
            <Select
              style={{ width: 150 }}
              value={rule.onStep}
              onChange={(value) => updateRule(index, { onStep: value })}
              options={draft.config.steps.map((step) => ({
                value: step.key,
                label: `在 ${step.name}`,
              }))}
            />
            <Text>之后，若</Text>
            <Input
              style={{ width: 130 }}
              placeholder="字段名"
              value={rule.when.field}
              onChange={(event) =>
                updateRule(index, { when: { ...rule.when, field: event.target.value } })
              }
            />
            <Select
              style={{ width: 100 }}
              value={rule.when.op}
              onChange={(value) => updateRule(index, { when: { ...rule.when, op: value } })}
              options={CONDITION_OPS}
            />
            <Input
              style={{ width: 130 }}
              placeholder="比较值"
              value={String(rule.when.value ?? '')}
              disabled={rule.when.op === 'empty' || rule.when.op === 'notEmpty'}
              onChange={(event) =>
                updateRule(index, { when: { ...rule.when, value: event.target.value } })
              }
            />
            <Text>则进入</Text>
            <Select
              style={{ width: 180 }}
              value={rule.goto}
              onChange={(value) => updateRule(index, { goto: value })}
              options={stepTargetOptions}
            />
            <Button
              type="text"
              danger
              icon={<DeleteOutlined />}
              onClick={() =>
                patchConfig({ rules: draft.config.rules.filter((_, i) => i !== index) })
              }
            />
          </Space>
        ))}

        <Button
          type="dashed"
          icon={<PlusOutlined />}
          onClick={() =>
            patchConfig({
              rules: [
                ...draft.config.rules,
                blankRule(draft.config.steps[0]?.key ?? ''),
              ],
            })
          }
          block
        >
          添加条件
        </Button>
      </Space>
    </Modal>
  );
}

interface StepEditorProps {
  step: StepConfig;
  index: number;
  total: number;
  roleOptions: { value: string; label: string }[];
  userOptions: { value: number; label: string }[];
  stepTargetOptions: { value: string; label: string }[];
  onChange: (patch: Partial<StepConfig>) => void;
  onMove: (delta: number) => void;
  onRemove: () => void;
}

function StepEditor({
  step,
  index,
  total,
  roleOptions,
  userOptions,
  stepTargetOptions,
  onChange,
  onMove,
  onRemove,
}: StepEditorProps) {
  const mode = step.assignee?.mode ?? 'initiator';

  return (
    <Space orientation="vertical" size="small" style={{ width: '100%' }}>
      <Space wrap>
        <Input
          style={{ width: 90 }}
          addonBefore="标识"
          value={step.key}
          onChange={(event) => onChange({ key: event.target.value })}
          disabled={index === 0}
        />
        <Input
          style={{ width: 200 }}
          addonBefore="名称"
          value={step.name}
          onChange={(event) => onChange({ name: event.target.value })}
        />
        <Select
          style={{ width: 130 }}
          value={step.kind}
          onChange={(value: StepKind) => onChange({ kind: value })}
          options={STEP_KINDS.map((item) => ({ value: item.value, label: item.label }))}
        />
        <Space size={0}>
          <Button
            type="text"
            icon={<ArrowUpOutlined />}
            disabled={index === 0}
            onClick={() => onMove(-1)}
          />
          <Button
            type="text"
            icon={<ArrowDownOutlined />}
            disabled={index === total - 1}
            onClick={() => onMove(1)}
          />
          <Popconfirm
            title="删除这个步骤？"
            okText="删除"
            cancelText="取消"
            onConfirm={onRemove}
            disabled={index === 0}
          >
            <Button type="text" danger icon={<DeleteOutlined />} disabled={index === 0} />
          </Popconfirm>
        </Space>
      </Space>

      <Text type="secondary">
        {STEP_KINDS.find((item) => item.value === step.kind)?.hint}
      </Text>

      {step.kind !== 'start' ? (
        <Space wrap align="center">
          <Text>由谁处理</Text>
          <Select
            style={{ width: 170 }}
            value={mode}
            onChange={(value: Assignee['mode']) => onChange({ assignee: defaultAssignee(value) })}
            options={[
              { value: 'assigned', label: '上一步分派的人' },
              { value: 'role', label: '按角色' },
              { value: 'users', label: '指定人员' },
              { value: 'initiator', label: '发起人本人' },
            ]}
          />
          {mode === 'role' ? (
            <Select
              mode="multiple"
              style={{ minWidth: 220 }}
              placeholder="选择角色"
              value={(step.assignee as { roles: string[] }).roles}
              onChange={(roles) => onChange({ assignee: { mode: 'role', roles } })}
              options={roleOptions}
            />
          ) : null}
          {mode === 'users' ? (
            <Select
              mode="multiple"
              style={{ minWidth: 260 }}
              placeholder="选择人员"
              value={(step.assignee as { userIds: number[] }).userIds}
              onChange={(userIds) => onChange({ assignee: { mode: 'users', userIds } })}
              options={userOptions}
              optionFilterProp="label"
            />
          ) : null}
        </Space>
      ) : null}

      {step.kind === 'handle' ? (
        <Space wrap align="center">
          <Text>多人时</Text>
          <Radio.Group
            value={step.completeRule}
            onChange={(event) => onChange({ completeRule: event.target.value })}
          >
            <Radio.Button value="all">全部完成才推进</Radio.Button>
            <Radio.Button value="any">任一人完成就推进</Radio.Button>
          </Radio.Group>
        </Space>
      ) : null}

      {step.kind === 'confirm' ? (
        <Space wrap align="center">
          <Text>打回到</Text>
          <Select
            style={{ width: 200 }}
            placeholder="选择打回的目标步骤"
            value={step.onReject ?? undefined}
            onChange={(value) => onChange({ onReject: value })}
            options={stepTargetOptions.filter((option) => option.value !== END)}
          />
        </Space>
      ) : null}

      {step.kind !== 'confirm' ? (
        <Space wrap align="center">
          <Text>完成后</Text>
          <Select
            style={{ width: 200 }}
            value={step.next ?? END}
            onChange={(value) => onChange({ next: value === END ? null : value })}
            options={stepTargetOptions}
          />
          <Text type="secondary">留默认则按步骤顺序走</Text>
        </Space>
      ) : null}

      <Text type="secondary">{assigneeLabel(step.assignee)}</Text>
    </Space>
  );
}
