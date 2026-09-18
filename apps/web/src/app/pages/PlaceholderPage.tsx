import { Card, Empty, Typography } from 'antd';

/** 尚未实现的功能占位，避免菜单点进来是空白页 */
export default function PlaceholderPage({
  title,
  description,
}: {
  title: string;
  description: string;
}) {
  return (
    <Card title={title}>
      <Empty
        image={Empty.PRESENTED_IMAGE_SIMPLE}
        description={
          <Typography.Text type="secondary" className="placeholder-desc">
            {description}
          </Typography.Text>
        }
      />
    </Card>
  );
}
