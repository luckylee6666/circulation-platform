import { Button, Card, Col, Result, Row, Space, Statistic, Table, Typography } from 'antd';
import type { TableColumnsType } from 'antd';
import { useNavigate } from 'react-router-dom';

import type { CommitOutcome, PreviewRow } from '@/shared/api/types';

interface Props {
  outcome: CommitOutcome;
  onRestart: () => void;
}

export default function ResultStep({ outcome, onRestart }: Props) {
  const navigate = useNavigate();
  const failed = outcome.failed;

  const columns: TableColumnsType<PreviewRow> = [
    { title: '行号', dataIndex: 'rowNumber', key: 'rowNumber', width: 70 },
    {
      title: '唯一键',
      dataIndex: 'extKey',
      key: 'extKey',
      render: (value: string) =>
        value.startsWith('auto-') ? (
          <Typography.Text type="secondary">自动生成</Typography.Text>
        ) : (
          value
        ),
    },
    {
      title: '失败原因',
      dataIndex: 'errors',
      key: 'errors',
      render: (errors: string[]) => <Typography.Text type="danger">{errors.join('；')}</Typography.Text>,
    },
  ];

  return (
    <Space orientation="vertical" size="large" style={{ width: '100%' }}>
      <Result
        status={failed > 0 ? 'warning' : 'success'}
        title={failed > 0 ? '导入完成，但有部分行被跳过' : '导入完成'}
        subTitle={`批次号 ${outcome.batchId} · 共处理 ${outcome.total} 行`}
        extra={
          <Space>
            <Button type="primary" onClick={() => navigate('/records')}>
              查看数据
            </Button>
            <Button onClick={onRestart}>再导一批</Button>
          </Space>
        }
      />

      <Row gutter={16}>
        <Col span={6}>
          <Card size="small">
            <Statistic
              title="新增"
              value={outcome.inserted}
              styles={{ content: { color: '#16a34a' } }}
            />
          </Card>
        </Col>
        <Col span={6}>
          <Card size="small">
            <Statistic
              title="更新"
              value={outcome.updated}
              styles={{ content: { color: '#1d4ed8' } }}
            />
          </Card>
        </Col>
        <Col span={6}>
          <Card size="small">
            <Statistic title="跳过" value={outcome.skipped} />
          </Card>
        </Col>
        <Col span={6}>
          <Card size="small">
            <Statistic
              title="失败"
              value={failed}
              styles={{ content: { color: failed > 0 ? '#dc2626' : undefined } }}
            />
          </Card>
        </Col>
      </Row>

      {outcome.errors.length > 0 ? (
        <Card title="未导入的行" size="small">
          <Table<PreviewRow>
            size="small"
            rowKey="rowNumber"
            pagination={{ pageSize: 10, showSizeChanger: false, hideOnSinglePage: true }}
            columns={columns}
            dataSource={outcome.errors}
          />
        </Card>
      ) : null}
    </Space>
  );
}
