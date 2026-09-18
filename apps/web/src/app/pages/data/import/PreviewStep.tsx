import { Alert, Button, Card, Col, Row, Space, Statistic, Table, Tag, Typography } from 'antd';
import type { TableColumnsType } from 'antd';
import { CheckCircleOutlined, CloseCircleOutlined, EditOutlined, MinusCircleOutlined } from '@ant-design/icons';

import type { FieldDef, ImportPreview, PreviewRow, RowStatus } from '@/shared/api/types';

interface Props {
  preview: ImportPreview;
  fields: FieldDef[];
  committing: boolean;
  onCommit: () => void;
  onBack: () => void;
}

const STATUS_META: Record<RowStatus, { label: string; color: string; icon: React.ReactNode }> = {
  insert: { label: '新增', color: 'green', icon: <CheckCircleOutlined /> },
  update: { label: '更新', color: 'blue', icon: <EditOutlined /> },
  skip: { label: '跳过', color: 'default', icon: <MinusCircleOutlined /> },
  invalid: { label: '错误', color: 'red', icon: <CloseCircleOutlined /> },
};

export default function PreviewStep({ preview, fields, committing, onCommit, onBack }: Props) {
  const { summary, rows, mappingErrors } = preview;
  const visibleFields = fields.filter((field) => field.enabled);

  const columns: TableColumnsType<PreviewRow> = [
    {
      title: '行号',
      dataIndex: 'rowNumber',
      key: 'rowNumber',
      width: 70,
    },
    {
      title: '结果',
      dataIndex: 'status',
      key: 'status',
      width: 90,
      render: (status: RowStatus) => {
        const meta = STATUS_META[status];
        return (
          <Tag color={meta.color} icon={meta.icon}>
            {meta.label}
          </Tag>
        );
      },
    },
    ...visibleFields.map((field) => ({
      title: field.label,
      key: field.code,
      ellipsis: true,
      render: (_: unknown, row: PreviewRow) => renderValue(row.data[field.code]),
    })),
    {
      title: '问题',
      dataIndex: 'errors',
      key: 'errors',
      render: (errors: string[]) =>
        errors.length === 0 ? (
          <Typography.Text type="secondary">—</Typography.Text>
        ) : (
          <Typography.Text type="danger">{errors.join('；')}</Typography.Text>
        ),
    },
  ];

  return (
    <Space orientation="vertical" size="large" style={{ width: '100%' }}>
      <Row gutter={16}>
        <Col span={6}>
          <Card size="small">
            <Statistic title="总行数" value={summary.total} />
          </Card>
        </Col>
        <Col span={6}>
          <Card size="small">
            <Statistic
              title="新增"
              value={summary.insert}
              styles={{ content: { color: '#16a34a' } }}
            />
          </Card>
        </Col>
        <Col span={6}>
          <Card size="small">
            <Statistic
              title="更新"
              value={summary.update}
              styles={{ content: { color: '#1d4ed8' } }}
            />
          </Card>
        </Col>
        <Col span={6}>
          <Card size="small">
            <Statistic
              title="错误"
              value={summary.invalid}
              styles={{ content: { color: summary.invalid > 0 ? '#dc2626' : undefined } }}
            />
          </Card>
        </Col>
      </Row>

      {mappingErrors.length > 0 ? (
        <Alert
          type="error"
          showIcon
          title="映射还有问题，无法导入"
          description={
            <ul className="import-error-list">
              {mappingErrors.map((error) => (
                <li key={error}>{error}</li>
              ))}
            </ul>
          }
        />
      ) : null}

      {summary.invalid > 0 ? (
        <Alert
          type="warning"
          showIcon
          title={`有 ${summary.invalid} 行数据有问题，这些行会被跳过，其余正常导入`}
          description="也可以点「上一步」调整映射后重新预览；或者先在 Excel 里改好这几行再传一次。"
        />
      ) : null}

      {summary.skip > 0 ? (
        <Alert type="info" showIcon title={`有 ${summary.skip} 行库里已存在，按你的选择将被跳过`} />
      ) : null}

      <Table<PreviewRow>
        size="medium"
        rowKey="rowNumber"
        loading={false}
        pagination={{ pageSize: 20, showSizeChanger: false, hideOnSinglePage: true }}
        scroll={{ x: 'max-content' }}
        columns={columns}
        dataSource={rows}
        rowClassName={(row) => (row.status === 'invalid' ? 'import-row-invalid' : '')}
      />

      <Space>
        <Button onClick={onBack}>上一步</Button>
        <Button
          type="primary"
          size="large"
          loading={committing}
          disabled={mappingErrors.length > 0 || summary.insert + summary.update === 0}
          onClick={onCommit}
        >
          确认导入（新增 {summary.insert} 条，更新 {summary.update} 条）
        </Button>
      </Space>
    </Space>
  );
}

function renderValue(value: unknown): string {
  if (value === null || value === undefined || value === '') return '—';
  if (typeof value === 'boolean') return value ? '是' : '否';
  return String(value);
}
