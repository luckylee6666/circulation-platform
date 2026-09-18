import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState } from 'react';
import type { ReactNode } from 'react';
import { useQueryClient } from '@tanstack/react-query';

import { notificationsApi } from '@/shared/api/endpoints';
import type { NotificationItem } from '@/shared/api/types';

/**
 * 推送正常时的兜底轮询间隔。
 *
 * 推送负责「快」，兜底负责「一定送达」：即使内网代理把长连接缓冲住了、
 * 推送静默失效，最坏情况下一分钟内也会自己拉回来。
 */
const PUSH_BACKSTOP_MS = 60_000;

/** 推送通道连不上时退回的轮询间隔 */
const FALLBACK_POLL_MS = 15_000;

/** 连续失败多少次就认定推送不可用 */
const MAX_PUSH_FAILURES = 3;

/**
 * 一声短促的双音提示。
 *
 * 用 Web Audio 现场合成而不是塞一个音频文件：省一个静态资源，
 * 也不用担心打包时路径对不上。浏览器要求先有用户交互才允许出声，
 * 所以首次点击/按键时会把音频上下文解锁。
 */
function useChime() {
  const contextRef = useRef<AudioContext | null>(null);

  useEffect(() => {
    const unlock = () => {
      void contextRef.current?.resume().catch(() => undefined);
    };
    window.addEventListener('pointerdown', unlock);
    window.addEventListener('keydown', unlock);
    return () => {
      window.removeEventListener('pointerdown', unlock);
      window.removeEventListener('keydown', unlock);
    };
  }, []);

  return useCallback(() => {
    try {
      const Ctor = window.AudioContext;
      if (!Ctor) return;
      contextRef.current ??= new Ctor();
      const context = contextRef.current;
      if (context.state === 'suspended') void context.resume().catch(() => undefined);

      const start = context.currentTime;
      const gain = context.createGain();
      gain.connect(context.destination);
      gain.gain.setValueAtTime(0.0001, start);
      gain.gain.exponentialRampToValueAtTime(0.18, start + 0.02);
      gain.gain.exponentialRampToValueAtTime(0.0001, start + 0.45);

      for (const [index, frequency] of [880, 1174].entries()) {
        const oscillator = context.createOscillator();
        oscillator.type = 'sine';
        oscillator.frequency.setValueAtTime(frequency, start + index * 0.11);
        oscillator.connect(gain);
        oscillator.start(start + index * 0.11);
        oscillator.stop(start + index * 0.11 + 0.24);
      }
    } catch {
      // 播不出声不影响功能，静默忽略
    }
  }, []);
}

interface NotifyContextValue {
  unreadCount: number;
  items: NotificationItem[];
  loading: boolean;
  markRead: (ids?: number[]) => Promise<void>;
  clearAll: () => Promise<void>;
  refresh: () => Promise<void>;
}

const NotifyContext = createContext<NotifyContextValue | null>(null);

export function NotifyProvider({ children, enabled }: { children: ReactNode; enabled: boolean }) {
  const queryClient = useQueryClient();
  const chime = useChime();

  const [items, setItems] = useState<NotificationItem[]>([]);
  const [unreadCount, setUnreadCount] = useState(0);
  const [loading, setLoading] = useState(false);
  /** 推送通道不可用时退回定时轮询 */
  const [pushDown, setPushDown] = useState(false);

  // 已经提醒过的最新消息 id，避免重复拉取时反复响铃
  const alertedRef = useRef<number | null>(null);

  const refresh = useCallback(async () => {
    if (!enabled) return;
    setLoading(true);
    try {
      const [list, count] = await Promise.all([
        notificationsApi.list(false, 30),
        notificationsApi.unreadCount(),
      ]);
      setItems(list);
      setUnreadCount(count.count);

      const newestUnread = list.find((item) => item.readAt === null)?.id ?? 0;
      if (alertedRef.current === null) {
        // 首次载入只记基线，不响铃——否则每次打开页面都会响
        alertedRef.current = newestUnread;
      } else if (newestUnread > alertedRef.current) {
        alertedRef.current = newestUnread;
        chime();
      }
    } catch {
      // 网络抖动或会话过期时静默跳过，下一轮会重试
    } finally {
      setLoading(false);
    }
  }, [enabled, chime]);

  useEffect(() => {
    if (!enabled) return;
    void refresh();

    const interval = pushDown ? FALLBACK_POLL_MS : PUSH_BACKSTOP_MS;
    const timer = window.setInterval(() => {
      void refresh();
      queryClient.invalidateQueries({ queryKey: ['flow-tasks'] });
      queryClient.invalidateQueries({ queryKey: ['flow-instances'] });
    }, interval);

    return () => window.clearInterval(timer);
  }, [enabled, refresh, pushDown, queryClient]);

  // 推送通道：能连上就用它拿到秒级的提醒，连不上就交给上面的轮询
  useEffect(() => {
    if (!enabled || pushDown) return;

    let failures = 0;
    const source = new EventSource('/api/events');

    source.onopen = () => {
      // 只要握手成功过就重置计数，偶发抖动不会触发降级
      failures = 0;
    };

    source.addEventListener('change', () => {
      void refresh();
      queryClient.invalidateQueries({ queryKey: ['flow-tasks'] });
      queryClient.invalidateQueries({ queryKey: ['flow-instances'] });
      queryClient.invalidateQueries({ queryKey: ['flow-instance'] });
    });

    source.onerror = () => {
      failures += 1;
      if (failures >= MAX_PUSH_FAILURES) {
        source.close();
        setPushDown(true);
      }
    };

    // 页面被导航销毁时主动断开。不做这一步的话，浏览器会把这些长连接留在
    // 连接池里，反复刷新或切换账号后很快堆到每主机上限，后续请求全部排队。
    const release = () => source.close();
    window.addEventListener('beforeunload', release);
    window.addEventListener('pagehide', release);

    return () => {
      window.removeEventListener('beforeunload', release);
      window.removeEventListener('pagehide', release);
      source.close();
    };
  }, [enabled, pushDown, refresh, queryClient]);

  // 从别的标签页切回来时立刻拉一次，不用干等下一个周期
  useEffect(() => {
    if (!enabled) return;
    const onFocus = () => void refresh();
    window.addEventListener('focus', onFocus);
    return () => window.removeEventListener('focus', onFocus);
  }, [enabled, refresh]);

  const markRead = useCallback(
    async (ids: number[] = []) => {
      await notificationsApi.markRead(ids);
      await refresh();
    },
    [refresh],
  );

  const clearAll = useCallback(async () => {
    await notificationsApi.clear();
    await refresh();
  }, [refresh]);

  const value = useMemo<NotifyContextValue>(
    () => ({ unreadCount, items, loading, markRead, clearAll, refresh }),
    [unreadCount, items, loading, markRead, clearAll, refresh],
  );

  return <NotifyContext.Provider value={value}>{children}</NotifyContext.Provider>;
}

export function useNotify(): NotifyContextValue {
  const value = useContext(NotifyContext);
  if (!value) {
    throw new Error('useNotify 必须在 NotifyProvider 内使用');
  }
  return value;
}
