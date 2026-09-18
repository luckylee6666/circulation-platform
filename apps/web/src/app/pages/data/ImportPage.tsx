import { useCallback, useState } from 'react';
import { App as AntApp, Card, Steps, Typography } from 'antd';
import { useQuery } from '@tanstack/react-query';

import MappingStep from './import/MappingStep';
import PreviewStep from './import/PreviewStep';
import ResultStep from './import/ResultStep';
import UploadStep from './import/UploadStep';
import { describeError } from '@/shared/api/client';
import { fieldsApi, importsApi } from '@/shared/api/endpoints';
import type {
  CommitOutcome,
  DedupeStrategy,
  FieldDef,
  ImportPreview,
  UploadResult,
} from '@/shared/api/types';

type Stage = 'upload' | 'mapping' | 'preview' | 'result';

const STEPS = [
  { title: '选择数据' },
  { title: '对应字段' },
  { title: '预览校验' },
  { title: '完成' },
];

/** 按字段名称/标识自动配对，改表头行时代替服务端重新猜一次 */
function suggestMapping(headers: string[], fields: FieldDef[]): Record<string, number> {
  const normalize = (value: string) =>
    value.replace(/[\s_\-:：()（）]/g, '').toLowerCase();
  const normalizedHeaders = headers.map(normalize);
  const mapping: Record<string, number> = {};

  for (const field of fields) {
    const index = normalizedHeaders.indexOf(normalize(field.label));
    if (index >= 0) mapping[field.code] = index;
  }
  for (const field of fields) {
    if (mapping[field.code] !== undefined) continue;
    const index = normalizedHeaders.indexOf(normalize(field.code));
    if (index >= 0) mapping[field.code] = index;
  }

  return mapping;
}

export default function ImportPage() {
  const { message } = AntApp.useApp();

  const [stage, setStage] = useState<Stage>('upload');
  const [busy, setBusy] = useState(false);
  const [upload, setUpload] = useState<UploadResult | null>(null);
  const [headerRow, setHeaderRow] = useState(0);
  const [mapping, setMapping] = useState<Record<string, number>>({});
  const [dedupe, setDedupe] = useState<DedupeStrategy>('upsert');
  const [preview, setPreview] = useState<ImportPreview | null>(null);
  const [outcome, setOutcome] = useState<CommitOutcome | null>(null);

  const fields = useQuery({ queryKey: ['fields'], queryFn: fieldsApi.list });
  const activeFields = (fields.data ?? []).filter((field) => field.enabled);

  const applyUpload = useCallback(
    (result: UploadResult) => {
      setUpload(result);
      setHeaderRow(result.headerRow);
      setMapping(result.suggestedMapping);
      // 没有唯一键时服务端会拒绝覆盖更新，这里直接把默认值调成「全部新增」
      setDedupe(result.uniqueKeyLabel ? 'upsert' : 'insert');
      setPreview(null);
      setOutcome(null);
      setStage('mapping');
    },
    [],
  );

  const handleFile = useCallback(
    async (file: File) => {
      setBusy(true);
      try {
        applyUpload(await importsApi.upload(file));
      } catch (error) {
        message.error(describeError(error));
      } finally {
        setBusy(false);
      }
    },
    [applyUpload, message],
  );

  const handlePaste = useCallback(
    async (text: string) => {
      setBusy(true);
      try {
        applyUpload(await importsApi.paste(text));
      } catch (error) {
        message.error(describeError(error));
      } finally {
        setBusy(false);
      }
    },
    [applyUpload, message],
  );

  const handleHeaderRowChange = useCallback(
    (index: number) => {
      setHeaderRow(index);
      const headers = upload?.previewRows[index] ?? [];
      setMapping(suggestMapping(headers, activeFields));
    },
    [upload, activeFields],
  );

  const handlePreview = useCallback(async () => {
    if (!upload) return;
    setBusy(true);
    try {
      const result = await importsApi.preview(upload.sessionId, { headerRow, mapping, dedupe });
      setPreview(result);
      setStage('preview');
    } catch (error) {
      message.error(describeError(error));
    } finally {
      setBusy(false);
    }
  }, [upload, headerRow, mapping, dedupe, message]);

  const handleCommit = useCallback(async () => {
    if (!upload) return;
    setBusy(true);
    try {
      const result = await importsApi.commit(upload.sessionId, { headerRow, mapping, dedupe });
      setOutcome(result);
      setStage('result');
    } catch (error) {
      message.error(describeError(error));
    } finally {
      setBusy(false);
    }
  }, [upload, headerRow, mapping, dedupe, message]);

  const restart = useCallback(() => {
    setStage('upload');
    setUpload(null);
    setPreview(null);
    setOutcome(null);
    setMapping({});
    setHeaderRow(0);
  }, []);

  const current = { upload: 0, mapping: 1, preview: 2, result: 3 }[stage];

  return (
    <Card title="导入数据">
      <Typography.Paragraph type="secondary">
        从外部系统导出的 Excel 或 CSV 导入进来。第一次需要把列对应到系统字段，
        对应关系会被记住，同样的表格下次可以一键导入。
      </Typography.Paragraph>

      <Steps className="import-steps" size="small" current={current} items={STEPS} />

      <div className="import-body">
        {stage === 'upload' ? (
          <UploadStep loading={busy} onFile={handleFile} onPaste={handlePaste} />
        ) : null}

        {stage === 'mapping' && upload ? (
          <MappingStep
            upload={upload}
            fields={activeFields}
            headerRow={headerRow}
            mapping={mapping}
            dedupe={dedupe}
            previewing={busy}
            onHeaderRowChange={handleHeaderRowChange}
            onMappingChange={setMapping}
            onDedupeChange={setDedupe}
            onPreview={handlePreview}
            onBack={restart}
          />
        ) : null}

        {stage === 'preview' && preview ? (
          <PreviewStep
            preview={preview}
            fields={activeFields}
            committing={busy}
            onCommit={handleCommit}
            onBack={() => setStage('mapping')}
          />
        ) : null}

        {stage === 'result' && outcome ? (
          <ResultStep outcome={outcome} onRestart={restart} />
        ) : null}
      </div>
    </Card>
  );
}
