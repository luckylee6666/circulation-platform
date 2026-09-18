import { useState } from 'react';
import { Button, Divider, Input, Space, Upload } from 'antd';
import { InboxOutlined } from '@ant-design/icons';

interface Props {
  loading: boolean;
  onFile: (file: File) => void;
  onPaste: (text: string) => void;
}

export default function UploadStep({ loading, onFile, onPaste }: Props) {
  const [text, setText] = useState('');

  return (
    <Space orientation="vertical" size="large" style={{ width: '100%' }}>
      <Upload.Dragger
        accept=".xlsx,.xls,.xlsm,.csv,.txt"
        showUploadList={false}
        maxCount={1}
        disabled={loading}
        beforeUpload={(file) => {
          onFile(file);
          // 交给自己的接口上传，不走 antd 默认的 action
          return Upload.LIST_IGNORE;
        }}
      >
        <p className="ant-upload-drag-icon">
          <InboxOutlined />
        </p>
        <p className="ant-upload-text">把 Excel 或 CSV 文件拖到这里</p>
        <p className="ant-upload-hint">
          也可以点击选择文件。单次不超过 20MB、10 万行；CSV 支持 UTF-8 与 GBK 编码
        </p>
      </Upload.Dragger>

      <Divider plain>或者直接从 Excel 复制粘贴</Divider>

      <Input.TextArea
        rows={6}
        value={text}
        disabled={loading}
        onChange={(event) => setText(event.target.value)}
        placeholder={'在 Excel 里选中数据区域，Ctrl+C 复制后粘贴到这里（记得带上表头行）'}
      />

      <Button
        type="primary"
        size="large"
        loading={loading}
        disabled={!text.trim()}
        onClick={() => onPaste(text)}
      >
        解析粘贴的内容
      </Button>
    </Space>
  );
}
