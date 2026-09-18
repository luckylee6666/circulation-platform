import { useEffect } from 'react';
import { useQuery } from '@tanstack/react-query';

import { siteApi } from '@/shared/api/endpoints';

/** 拿不到配置时的兜底名称，保证界面不会出现空白标题 */
const FALLBACK_NAME = '流转平台';

/**
 * 当前平台名称，由管理员在桌面控制台里配置。
 *
 * 做成普通 hook 而不是 Context Provider：React Query 本身就按 queryKey 去重，
 * 多个组件同时调用只会发一次请求，没必要再包一层。
 */
export function useSiteName(): string {
  const site = useQuery({
    queryKey: ['site'],
    queryFn: siteApi.read,
    staleTime: 5 * 60 * 1000,
    retry: false,
  });

  const name = site.data?.name?.trim() || FALLBACK_NAME;

  // 浏览器标签页标题也要跟着变，否则用户看到的还是默认名字
  useEffect(() => {
    if (document.title !== name) {
      document.title = name;
    }
  }, [name]);

  return name;
}
