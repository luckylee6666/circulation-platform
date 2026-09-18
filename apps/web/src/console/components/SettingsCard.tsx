import { useEffect, useState } from 'react';
import { Button, Card, Form, InputNumber, Space, Switch, Typography } from 'antd';

import type { Settings } from '../types';

interface Props {
  settings: Settings;
  serviceRunning: boolean;
  saving: boolean;
  onSave: (settings: Settings) => void;
}

export default function SettingsCard({ settings, serviceRunning, saving, onSave }: Props) {
  const [draft, setDraft] = useState<Settings>(settings);

  useEffect(() => {
    setDraft(settings);
  }, [settings]);

  const portChanged = draft.port !== settings.port;

  return (
    <Card title="设置">
      <Form layout="vertical" className="settings-form">
        <Form.Item
          label="服务端口"
          extra={
            portChanged && serviceRunning
              ? '端口修改后会停止当前服务，需要手动重新启动'
              : '局域网用户通过这个端口访问，默认 8080'
          }
        >
          <InputNumber
            min={1024}
            max={65535}
            value={draft.port}
            onChange={(value) => setDraft({ ...draft, port: Number(value) || 8080 })}
          />
        </Form.Item>

        <Form.Item label="开机自动启动">
          <Space>
            <Switch
              checked={draft.autostart}
              onChange={(checked) => setDraft({ ...draft, autostart: checked })}
            />
            <Typography.Text type="secondary">
              随系统启动，适合把本机长期当服务器用
            </Typography.Text>
          </Space>
        </Form.Item>

        <Form.Item label="启动软件时自动开启服务">
          <Space>
            <Switch
              checked={draft.startServiceOnLaunch}
              onChange={(checked) => setDraft({ ...draft, startServiceOnLaunch: checked })}
            />
            <Typography.Text type="secondary">关掉后需要手动点「启动服务」</Typography.Text>
          </Space>
        </Form.Item>

        <Button type="primary" loading={saving} onClick={() => onSave(draft)}>
          保存设置
        </Button>
      </Form>
    </Card>
  );
}
