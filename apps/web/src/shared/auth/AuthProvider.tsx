import { createContext, useCallback, useContext, useEffect, useMemo } from 'react';
import type { ReactNode } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';

import { setUnauthorizedHandler } from '@/shared/api/client';
import { authApi } from '@/shared/api/endpoints';
import type { SessionInfo, User } from '@/shared/api/types';

interface AuthContextValue {
  user: User | null;
  roles: string[];
  permissions: string[];
  mustChangePassword: boolean;
  loading: boolean;
  can: (permission: string) => boolean;
  login: (username: string, password: string) => Promise<SessionInfo>;
  logout: () => Promise<void>;
  reload: () => Promise<void>;
}

const AuthContext = createContext<AuthContextValue | null>(null);

const ME_KEY = ['session', 'me'] as const;

export function AuthProvider({ children }: { children: ReactNode }) {
  const queryClient = useQueryClient();

  const session = useQuery({
    queryKey: ME_KEY,
    queryFn: authApi.me,
    // 未登录时 /auth/me 必然 401，重试没有意义
    retry: false,
    staleTime: 5 * 60 * 1000,
    refetchOnWindowFocus: false,
  });

  // 任何接口返回 401 都说明会话已失效，直接清空本地会话，界面会跳回登录页
  useEffect(() => {
    setUnauthorizedHandler(() => {
      queryClient.setQueryData(ME_KEY, null);
    });
    return () => setUnauthorizedHandler(null);
  }, [queryClient]);

  const login = useCallback(
    async (username: string, password: string) => {
      const info = await authApi.login(username, password);
      queryClient.setQueryData(ME_KEY, info);
      return info;
    },
    [queryClient],
  );

  const logout = useCallback(async () => {
    try {
      await authApi.logout();
    } finally {
      // 必须先 setQueryData 再清理其他缓存：clear() 会把 me 这个查询连同观察者一起移除，
      // 之后再写入就不会触发重渲染，界面会一直停在退出前的状态。
      queryClient.setQueryData(ME_KEY, null);
      queryClient.removeQueries({
        predicate: (query) => query.queryKey[0] !== ME_KEY[0],
      });
    }
  }, [queryClient]);

  const reload = useCallback(async () => {
    await queryClient.invalidateQueries({ queryKey: ME_KEY });
  }, [queryClient]);

  const value = useMemo<AuthContextValue>(() => {
    const data = session.data ?? null;
    const permissions = data?.permissions ?? [];

    return {
      user: data?.user ?? null,
      roles: data?.roles ?? [],
      permissions,
      mustChangePassword: data?.mustChangePassword ?? false,
      loading: session.isLoading,
      can: (permission: string) => permissions.includes(permission),
      login,
      logout,
      reload,
    };
  }, [session.data, session.isLoading, login, logout, reload]);

  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
}

export function useAuth(): AuthContextValue {
  const context = useContext(AuthContext);
  if (!context) {
    throw new Error('useAuth 必须在 AuthProvider 内部使用');
  }
  return context;
}
