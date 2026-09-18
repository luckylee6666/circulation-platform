import { useMemo } from 'react';
import { Alert, Button, Radio, Select, Space, Table, Tag, Typography } from 'antd';
import type { TableColumnsType } from 'antd';

import type { DedupeStrategy, FieldDef, UploadResult } from '@/shared/api/types';

interface Props {
  upload: UploadResult;
  fields: FieldDef[];
  headerRow: number;
  mapping: Record<string, number>;
  dedupe: DedupeStrategy;
  previewing: boolean;
  onHeaderRowChange: (index: number) => void;
  onMappingChange: (mapping: Record<string, number>) => void;
  onDedupeChange: (dedupe: DedupeStrategy) => void;
  onPreview: () => void;
  onBack: () => void;
}

interface RawRow {
  key: number;
  index: number;
  cells: string[];
}

export default function MappingStep({
  upload,
  fields,
  headerRow,
  mapping,
  dedupe,
  previewing,
  onHeaderRowChange,
  onMappingChange,
  onDedupeChange,
  onPreview,
  onBack,
}: Props) {
  const headers = upload.previewRows[headerRow] ?? [];

  const columnOptions = useMemo(
    () =>
      headers.map((header, index) => ({
        value: index,
        label: `${index + 1}. ${header.trim() || `第 ${index + 1} 列`}`,
      })),
    [headers],
  );

  const activeFields = fields.filter((field) => field.enabled);

  const missingRequired = activeFields.filter(
    (field) => field.required && mapping[field.code] === undefined,
  );
  const uniqueField = activeFields.find((field) => field.isUniqueKey);
  const uniqueUnmapped = uniqueField !== undefined && mapping[uniqueField.code] === undefined;

  const setField = (code: string, index: number | undefined) => {
    const next = { ...mapping };
    if (index === undefined) {
      delete next[code];
    } else {
      next[code] = index;
    }
    onMappingChange(next);
  };

  const rawColumns: TableColumnsType<RawRow> = [
    {
      title: '表头',
      key: 'pick',
      width: 70,
      render: (_, row) => (
        <Radio
          checked={row.index === headerRow}
          onChange={() => onHeaderRowChange(row.index)}
          aria-label={`选第 ${row.index + 1} 行为表头`}
        />
      ),
    },
    {
      title: '行',
      dataIndex: 'index',
      key: 'index',
      width: 60,
      render: (index: number) => (
        <Typography.Text type={index === headerRow ? undefined : 'secondary'}>
          {index + 1}
        </Typography.Text>
      ),
    },
    ...headers.map((_, columnIndex) => ({
      title: `第 ${columnIndex + 1} 列`,
      key: `col-${columnIndex}`,
      ellipsis: true,
      render: (_: unknown, row: RawRow) => row.cells[columnIndex] || '',
    })),
  ];

  const rawRows: RawRow[] = upload.previewRows.map((cells, index) => ({
    key: index,
    index,
    cells,
  }));

  return (
    <Space orientation="vertical" size="large" style={{ width: '100%' }}>
      {upload.matchedTemplate ? (
        <Alert
          type="success"
          showIcon
          title={`已套用上次保存的映射模板：${upload.matchedTemplate.name}`}
          description="如果这次的表格结构和上次一致，直接点「下一步：预览」就行。"
        />
      ) : null}

      <div>
        <Typography.Title level={5}>1. 确认表头在第几行</Typography.Title>
        <Typography.Paragraph type="secondary" className="import-hint">
          很多系统导出的表格前面有标题行，如果表头认错了，点左侧圆点换一行。
        </Typography.Paragraph>

        <Table<RawRow>
          className="import-raw-table"
          size="small"
          bordered
          rowKey="key"
          pagination={false}
          scroll={{ x: 'max-content' }}
          columns={rawColumns}
          dataSource={rawRows}
        />
        {upload.totalRows > upload.previewRows.length ? (
          <Typography.Text type="secondary" className="import-hint">
            仅显示前 {upload.previewRows.length} 行用于确认，共 {upload.totalRows} 行。
          </Typography.Text>
        ) : null}
      </div>

      <div>
        <Typography.Title level={5}>2. 把系统字段对应到表格的列</Typography.Title>
        <Typography.Paragraph type="secondary" className="import-hint">
          同名会自动配对；不需要导入的字段留空即可。
        </Typography.Paragraph>

        <div className="import-mapping">
          {activeFields.map((field) => (
            <div key={field.code} className="import-mapping-row">
              <div className="import-mapping-label">
                <Typography.Text>{field.label}</Typography.Text>
                {field.required ? <Tag color="red">必填</Tag> : null}
                {field.isUniqueKey ? <Tag color="blue">唯一键</Tag> : null}
                <Typography.Text type="secondary" className="import-mapping-code">
                  {field.code}
                </Typography.Text>
              </div>
              <Select
                allowClear
                className="import-mapping-select"
                placeholder="不导入"
                value={mapping[field.code]}
                options={columnOptions}
                onChange={(value) => setField(field.code, value)}
              />
            </div>
          ))}
        </div>
      </div>

      <div>
        <Typography.Title level={5}>3. 遇到重复数据怎么办</Typography.Title>
        {uniqueField ? (
          <>
            <Typography.Paragraph type="secondary" className="import-hint">
              按唯一键「{uniqueField.label}」判断库里是否已有这条数据。
            </Typography.Paragraph>
            <Radio.Group
              value={dedupe}
              onChange={(event) => onDedupeChange(event.target.value as DedupeStrategy)}
            >
              <Space orientation="vertical">
                <Radio value="upsert">覆盖更新：已有则用新数据覆盖</Radio>
                <Radio value="skip">跳过：已有则保留原数据不动</Radio>
                <Radio value="insert">全部新增：每条都当新数据插入</Radio>
              </Space>
            </Radio.Group>
          </>
        ) : (
          <Alert
            type="warning"
            showIcon
            title="还没有设置唯一键字段"
            description="只能「全部新增」。建议先到「字段定义」里把编号一类的字段标记为唯一键，这样才能按编号覆盖更新。"
          />
        )}
      </div>

      {missingRequired.length > 0 || uniqueUnmapped ? (
        <Alert
          type="error"
          showIcon
          title="还有必填项没有对应列"
          description={
            <ul className="import-error-list">
              {missingRequired.map((field) => (
                <li key={field.code}>必填字段「{field.label}」没有选择对应的列</li>
              ))}
              {uniqueUnmapped && uniqueField ? (
                <li key="unique">唯一键「{uniqueField.label}」没有选择对应的列</li>
              ) : null}
            </ul>
          }
        />
      ) : null}

      <Space>
        <Button onClick={onBack}>上一步</Button>
        <Button
          type="primary"
          size="large"
          loading={previewing}
          disabled={missingRequired.length > 0 || uniqueUnmapped}
          onClick={onPreview}
        >
          下一步：预览校验结果
        </Button>
      </Space>
    </Space>
  );
}
