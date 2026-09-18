import { Card, Space, Typography } from 'antd';

import { formatBytes } from '../format';
import type { LoadPoint, ServiceStatus } from '../types';

const { Text } = Typography;

interface Props {
  status: ServiceStatus;
}

/**
 * 内存趋势。
 *
 * 只画一条线、不引图表库——控制台的体积已经不小了，
 * 而这里要看的是「有没有一直往上涨」，折线足够说明问题。
 */
function Sparkline({ points }: { points: LoadPoint[] }) {
  if (points.length < 2) {
    return (
      <div className="load-spark-empty">
        <Text type="secondary">运行两分钟后开始记录趋势</Text>
      </div>
    );
  }

  const values = points.map((point) => point.memoryBytes);
  const min = Math.min(...values);
  const max = Math.max(...values);
  // 全程平稳时避免除以 0，把线放中间
  const span = max - min || 1;
  const width = 100;
  const height = 100;

  const path = points
    .map((point, index) => {
      const x = (index / (points.length - 1)) * width;
      // 上下各留 8% 余量，免得线贴边
      const ratio = (point.memoryBytes - min) / span;
      const y = height - 8 - ratio * (height - 16);
      return `${index === 0 ? 'M' : 'L'}${x.toFixed(2)},${y.toFixed(2)}`;
    })
    .join(' ');

  return (
    <div className="load-spark-wrap">
      <svg
        className="load-spark"
        viewBox={`0 0 ${width} ${height}`}
        preserveAspectRatio="none"
        role="img"
        aria-label="内存占用趋势"
      >
        <path d={path} fill="none" stroke="#1d4ed8" strokeWidth={1.5} vectorEffect="non-scaling-stroke" />
      </svg>
      <div className="load-spark-axis">
        <Text type="secondary">{points[0].at}</Text>
        <Text type="secondary">
          最低 {formatBytes(min)} · 最高 {formatBytes(max)}
        </Text>
        <Text type="secondary">{points[points.length - 1].at}</Text>
      </div>
    </div>
  );
}

export default function LoadCard({ status }: Props) {
  const history = status.loadHistory ?? [];

  return (
    <Card title="服务负载">
      <div className="load-grid">
        <div className="load-item">
          <Text type="secondary">内存占用</Text>
          <div className="load-value">{formatBytes(status.memoryBytes)}</div>
        </div>
        <div className="load-item">
          <Text type="secondary">CPU 占用</Text>
          <div className="load-value">{status.cpuPercent.toFixed(1)}%</div>
        </div>
        <div className="load-item">
          <Text type="secondary">在线用户</Text>
          <div className="load-value">{status.running ? status.onlineUsers : '—'}</div>
          <Text type="secondary" className="load-note">
            {status.running ? `最近 5 分钟有操作 · 共 ${status.onlineSessions} 个登录凭证` : '服务未运行'}
          </Text>
        </div>
      </div>

      <div className="load-trend">
        <Space>
          <Text strong>内存趋势</Text>
          <Text type="secondary">近 1 小时，每 30 秒一个点</Text>
        </Space>
        <Sparkline points={history} />
      </div>
    </Card>
  );
}
