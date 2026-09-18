import { Drawer, Empty, Typography } from 'antd';

import type { AppInfo } from '../types';

interface Props {
  open: boolean;
  logs: string;
  info: AppInfo | null;
  onClose: () => void;
}

export default function LogDrawer({ open, logs, info, onClose }: Props) {
  return (
    <Drawer
      title="运行日志"
      placement="right"
      size="large"
      open={open}
      onClose={onClose}
      destroyOnHidden
    >
      {info ? (
        <Typography.Paragraph type="secondary" className="log-hint">
          日志目录：{info.logDir}
        </Typography.Paragraph>
      ) : null}

      {logs.trim() ? (
        <pre className="log-view">{logs}</pre>
      ) : (
        <Empty description="暂无日志" />
      )}
    </Drawer>
  );
}
