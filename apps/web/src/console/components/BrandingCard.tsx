import { useEffect, useState } from 'react';
import { App as AntApp, Button, Card, Form, Input, Typography } from 'antd';

import { api } from '../api';

const { Text } = Typography;

const MAX_CHARS = 24;

/**
 * 平台名称。局域网用户登录页、侧边栏和浏览器标签页显示的就是这个名字。
 *
 * 这里直接读写本地数据库，不走 HTTP——控制台是这个程序的本地管理面，
 * 而且服务停着的时候也应该能改配置。
 */
export default function BrandingCard() {
  const { message } = AntApp.useApp();
  const [saved, setSaved] = useState<string | null>(null);
  const [draft, setDraft] = useState('');
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    void api.getSiteName().then((name) => {
      setSaved(name);
      setDraft(name);
    });
  }, []);

  const changed = saved !== null && draft.trim() !== saved;

  const save = async () => {
    setSaving(true);
    try {
      const name = await api.saveSiteName(draft);
      setSaved(name);
      setDraft(name);
      void message.success('平台名称已更新，局域网用户刷新页面即可看到');
    } catch (error) {
      void message.error(String(error));
    } finally {
      setSaving(false);
    }
  };

  return (
    <Card title="平台名称">
      <Form layout="vertical" className="settings-form">
        <Form.Item
          label="用户看到的名称"
          extra={`显示在登录页、侧边栏和浏览器标签页上，最多 ${MAX_CHARS} 个字`}
        >
          <Input
            value={draft}
            maxLength={MAX_CHARS}
            showCount
            disabled={saved === null}
            placeholder="例如：某某办公平台"
            onChange={(event) => setDraft(event.target.value)}
            onPressEnter={() => changed && void save()}
          />
        </Form.Item>

        <Button type="primary" loading={saving} disabled={!changed} onClick={() => void save()}>
          保存名称
        </Button>

        {changed ? (
          <Text type="secondary" style={{ marginLeft: 12 }}>
            改动尚未保存
          </Text>
        ) : null}
      </Form>
    </Card>
  );
}
