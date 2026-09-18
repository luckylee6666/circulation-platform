import type { Assignee, StepKind } from '@/shared/api/types';

export const STEP_KINDS: { value: StepKind; label: string; hint: string }[] = [
  { value: 'start', label: '发起', hint: '流程起点，发起时自动通过，不产生待办' },
  { value: 'dispatch', label: '分派', hint: '由分派人选择接下来谁来承办' },
  { value: 'handle', label: '承办', hint: '具体干活的人，可以同时有多个' },
  { value: 'confirm', label: '确认', hint: '确认结果，可以通过或打回重做' },
];

export function stepKindLabel(kind: string): string {
  return STEP_KINDS.find((item) => item.value === kind)?.label ?? kind;
}

const INSTANCE_STATUS: Record<number, { label: string; color: string }> = {
  0: { label: '草稿', color: 'default' },
  1: { label: '进行中', color: 'processing' },
  2: { label: '已完成', color: 'success' },
  3: { label: '已终止', color: 'default' },
};

export function instanceStatus(status: number): { label: string; color: string } {
  return INSTANCE_STATUS[status] ?? { label: `未知(${status})`, color: 'default' };
}

export function assigneeLabel(assignee: Assignee): string {
  switch (assignee?.mode) {
    case 'role':
      return `角色：${assignee.roles.join('、') || '未选择'}`;
    case 'users':
      return `指定人员（${assignee.userIds.length} 人）`;
    case 'assigned':
      return '上一步分派的人';
    default:
      return '发起人本人';
  }
}

/** 步骤配置的一句话摘要，列表和卡片上用。 */
export function stepSummary(step: {
  kind: string;
  assignee: Assignee;
  completeRule: string;
  next?: string | null;
}): string {
  const parts = [assigneeLabel(step.assignee)];
  if (step.kind === 'handle' && step.completeRule === 'any') {
    parts.push('任一人完成即可推进');
  }
  if (step.next) {
    parts.push(`完成后 → ${step.next === 'end' ? '结束' : step.next}`);
  }
  return parts.join(' · ');
}
