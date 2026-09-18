import { useState } from 'react';
import {
  Card,
  Col,
  Descriptions,
  Empty,
  Row,
  Segmented,
  Space,
  Statistic,
  Table,
  Tag,
  Typography,
} from 'antd';
import type { TableColumnsType } from 'antd';
import { useQuery } from '@tanstack/react-query';
import {
  Bar,
  BarChart,
  CartesianGrid,
  Legend,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from 'recharts';

import { statsApi } from '@/shared/api/endpoints';
import type { AssigneeStat, StepStat, TrendPoint } from '@/shared/api/types';
import { useAuth } from '@/shared/auth/AuthProvider';

import { stepKindLabel } from './flows/labels';

const { Text } = Typography;

/** 时长统一按小时展示，不足一天时说小时，超过一天换算成天。 */
function formatHours(hours: number | null | undefined): string {
  if (hours === null || hours === undefined || Number.isNaN(hours)) return '—';
  if (hours < 1) return `${Math.round(hours * 60)} 分钟`;
  if (hours < 48) return `${hours.toFixed(1)} 小时`;
  return `${(hours / 24).toFixed(1)} 天`;
}

/** recharts 的横轴只显示月-日，30 个点才排得下 */
function shortDay(day: string): string {
  return day.slice(5);
}

export default function StatsPage() {
  const { can } = useAuth();
  const canSeeGlobal = can('stats:view');

  const [scope, setScope] = useState<'mine' | 'global'>(canSeeGlobal ? 'global' : 'mine');
  const [days, setDays] = useState(30);
  const [overdueHours, setOverdueHours] = useState(48);

  const mine = useQuery({
    queryKey: ['stats-mine', days, overdueHours],
    queryFn: () => statsApi.mine(days, overdueHours),
  });

  const report = useQuery({
    queryKey: ['stats-report', days, overdueHours],
    queryFn: () => statsApi.report(days, overdueHours),
    enabled: canSeeGlobal && scope === 'global',
  });

  const stepColumns: TableColumnsType<StepStat> = [
    {
      title: '环节',
      dataIndex: 'stepName',
      render: (name: string, step) => (
        <Space size={4}>
          <Tag>{stepKindLabel(step.stepType)}</Tag>
          {name}
        </Space>
      ),
    },
    { title: '产生任务', dataIndex: 'total', width: 100 },
    {
      title: '待处理',
      dataIndex: 'pending',
      width: 100,
      render: (value: number) => (value > 0 ? <Tag color="processing">{value}</Tag> : '—'),
    },
    { title: '已处理', dataIndex: 'done', width: 100 },
    {
      title: '平均停留',
      dataIndex: 'avgHours',
      width: 130,
      render: (value: number | null) => formatHours(value),
    },
  ];

  const assigneeColumns: TableColumnsType<AssigneeStat> = [
    { title: '处理人', dataIndex: 'displayName' },
    { title: '部门', dataIndex: 'dept', width: 140, render: (value: string) => value || '—' },
    {
      title: '待处理',
      dataIndex: 'pending',
      width: 100,
      render: (value: number) => (value > 0 ? <Tag color="processing">{value}</Tag> : '—'),
    },
    {
      title: '超期',
      dataIndex: 'overdue',
      width: 90,
      render: (value: number) => (value > 0 ? <Tag color="error">{value}</Tag> : '—'),
    },
    { title: '已处理', dataIndex: 'done', width: 100 },
    {
      title: '平均处理时长',
      dataIndex: 'avgHours',
      width: 140,
      render: (value: number | null) => formatHours(value),
    },
  ];

  const controls = (
    <Space size={12} wrap>
      <Segmented
        value={scope}
        onChange={(value) => setScope(value as 'mine' | 'global')}
        options={[
          { value: 'mine', label: '我的' },
          ...(canSeeGlobal ? [{ value: 'global', label: '全局' }] : []),
        ]}
      />
      <Segmented
        value={days}
        onChange={(value) => setDays(value as number)}
        options={[
          { value: 7, label: '近 7 天' },
          { value: 30, label: '近 30 天' },
          { value: 90, label: '近 90 天' },
        ]}
      />
      <Space size={6}>
        <Text type="secondary">超期阈值</Text>
        <Segmented
          value={overdueHours}
          onChange={(value) => setOverdueHours(value as number)}
          options={[
            { value: 24, label: '24 小时' },
            { value: 48, label: '48 小时' },
            { value: 72, label: '3 天' },
            { value: 168, label: '7 天' },
          ]}
        />
      </Space>
    </Space>
  );

  return (
    <Space orientation="vertical" size="middle" style={{ width: '100%' }}>
      <Card title="统计" extra={controls}>
        {scope === 'mine' ? (
          <Row gutter={[16, 16]}>
            <Col xs={12} md={6}>
              <Card size="small">
                <Statistic title="我的待办" value={mine.data?.pending ?? 0} />
              </Card>
            </Col>
            <Col xs={12} md={6}>
              <Card size="small">
                <Statistic
                  title="其中超期"
                  value={mine.data?.overdue ?? 0}
                  styles={{ content: { color: (mine.data?.overdue ?? 0) > 0 ? '#dc2626' : undefined } }}
                />
              </Card>
            </Col>
            <Col xs={12} md={6}>
              <Card size="small">
                <Statistic title="我已处理" value={mine.data?.done ?? 0} />
              </Card>
            </Col>
            <Col xs={12} md={6}>
              <Card size="small">
                <Statistic
                  title="平均处理时长"
                  value={formatHours(mine.data?.avgHours)}
                  styles={{ content: { fontSize: 20 } }}
                />
              </Card>
            </Col>
          </Row>
        ) : (
          <Row gutter={[16, 16]}>
            <Col xs={12} md={6}>
              <Card size="small">
                <Statistic title="进行中" value={report.data?.overview.running ?? 0} />
              </Card>
            </Col>
            <Col xs={12} md={6}>
              <Card size="small">
                <Statistic
                  title="超期未办"
                  value={report.data?.overview.overdue ?? 0}
                  styles={{
                    content: {
                      color: (report.data?.overview.overdue ?? 0) > 0 ? '#dc2626' : undefined,
                    },
                  }}
                />
              </Card>
            </Col>
            <Col xs={12} md={6}>
              <Card size="small">
                <Statistic title="今日新增" value={report.data?.overview.createdToday ?? 0} />
              </Card>
            </Col>
            <Col xs={12} md={6}>
              <Card size="small">
                <Statistic title="今日办结" value={report.data?.overview.finishedToday ?? 0} />
              </Card>
            </Col>
          </Row>
        )}
      </Card>

      <Card title={`${scope === 'mine' ? '我的' : '全库'}办理趋势`} size="small">
        <TrendChart
          points={scope === 'mine' ? (mine.data?.trend ?? []) : (report.data?.trend ?? [])}
          labels={
            scope === 'mine'
              ? { created: '我发起的', finished: '我处理的' }
              : { created: '新增事项', finished: '办结事项' }
          }
        />
      </Card>

      {scope === 'global' && report.data ? (
        <>
          <Card size="small">
            <Descriptions
              size="small"
              column={{ xs: 1, sm: 2, md: 4 }}
              items={[
                { key: 'total', label: '事项总数', children: report.data.overview.total },
                { key: 'finished', label: '已完成', children: report.data.overview.finished },
                { key: 'terminated', label: '已终止', children: report.data.overview.terminated },
                {
                  key: 'avg',
                  label: '平均办结时长',
                  children: formatHours(report.data.overview.avgFinishHours),
                },
                {
                  key: 'running',
                  label: '在办平均已耗时',
                  children: formatHours(report.data.overview.avgRunningHours),
                },
                {
                  key: 'overdue',
                  label: `超期口径`,
                  children: `当前环节停留超过 ${report.data.overview.overdueHours} 小时`,
                },
                {
                  key: 'records',
                  label: '数据条数',
                  children: report.data.data.records,
                },
                {
                  key: 'lastImport',
                  label: '最近导入',
                  children: report.data.data.lastImportAt
                    ? `${report.data.data.lastImportAt}（${report.data.data.lastImportRows} 条）`
                    : '还没有导入过',
                },
              ]}
            />
          </Card>

          <Card title="各环节情况" size="small">
            <Table
              rowKey="stepKey"
              size="small"
              columns={stepColumns}
              dataSource={report.data.steps}
              pagination={false}
            />
          </Card>

          <Card title="按人统计" size="small">
            <Table
              rowKey="userId"
              size="small"
              columns={assigneeColumns}
              dataSource={report.data.assignees}
              pagination={false}
              locale={{ emptyText: '还没有产生过任务' }}
            />
          </Card>
        </>
      ) : null}
    </Space>
  );
}

function TrendChart({
  points,
  labels,
}: {
  points: TrendPoint[];
  labels: { created: string; finished: string };
}) {
  if (points.length === 0) {
    return <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="暂无数据" />;
  }

  // 用柱状而不是面积：按天计数是离散的，画成连续曲线会暗示中间的空白也有量；
  // 而且只有一两天有数据时，柱子在图上看得见，折线会贴在边缘看不出来。
  // 关掉入场动画：看板不需要，还会让「刚打开时是空的」这种观感问题出现。
  return (
    <div style={{ width: '100%', height: 260 }}>
      <ResponsiveContainer>
        <BarChart data={points} margin={{ top: 8, right: 16, bottom: 0, left: -16 }}>
          <CartesianGrid strokeDasharray="3 3" stroke="#eef0f4" vertical={false} />
          <XAxis dataKey="day" tickFormatter={shortDay} tick={{ fontSize: 12 }} tickMargin={8} />
          <YAxis allowDecimals={false} tick={{ fontSize: 12 }} width={44} />
          <Tooltip cursor={{ fill: 'rgba(29, 78, 216, 0.06)' }} />
          <Legend />
          <Bar
            dataKey="created"
            name={labels.created}
            fill="#1d4ed8"
            radius={[3, 3, 0, 0]}
            isAnimationActive={false}
          />
          <Bar
            dataKey="finished"
            name={labels.finished}
            fill="#16a34a"
            radius={[3, 3, 0, 0]}
            isAnimationActive={false}
          />
        </BarChart>
      </ResponsiveContainer>
    </div>
  );
}
