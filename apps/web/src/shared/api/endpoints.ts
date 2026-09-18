import { request, requestRaw } from './client';
import type {
  CommitOutcome,
  CompleteTaskInput,
  CreateUserResult,
  FieldDef,
  FieldInput,
  FlowDef,
  FlowDefInput,
  FlowTask,
  ImportBatch,
  ImportPreview,
  ImportTemplate,
  InstanceDetail,
  InstanceItem,
  MappingRequest,
  MyStats,
  NotificationItem,
  Permission,
  RecordData,
  RecordItem,
  RecordPage,
  Role,
  RoleInput,
  SessionInfo,
  StatsReport,
  UploadResult,
  User,
  UserInput,
  UserOption,
} from './types';

export const authApi = {
  login: (username: string, password: string) =>
    request<SessionInfo>('/auth/login', { method: 'POST', body: { username, password } }),

  logout: () => request<{ ok: boolean }>('/auth/logout', { method: 'POST', body: {} }),

  me: () => request<SessionInfo>('/auth/me'),

  changePassword: (oldPassword: string, newPassword: string) =>
    request<{ ok: boolean }>('/auth/change-password', {
      method: 'POST',
      body: { oldPassword, newPassword },
    }),
};

export const usersApi = {
  list: (keyword = '') =>
    request<User[]>(`/users${keyword ? `?keyword=${encodeURIComponent(keyword)}` : ''}`),

  options: () => request<UserOption[]>('/users/options'),

  create: (input: UserInput) =>
    request<CreateUserResult>('/users', { method: 'POST', body: input }),

  update: (id: number, input: UserInput) =>
    request<{ ok: boolean }>(`/users/${id}`, { method: 'PUT', body: input }),

  resetPassword: (id: number, password: string) =>
    request<{ password: string }>(`/users/${id}/password`, {
      method: 'POST',
      body: { password },
    }),

  setStatus: (id: number, status: number) =>
    request<{ ok: boolean }>(`/users/${id}/status`, { method: 'POST', body: { status } }),

  remove: (id: number) => request<{ ok: boolean }>(`/users/${id}`, { method: 'DELETE' }),
};

export const rolesApi = {
  list: () => request<Role[]>('/roles'),
  permissions: () => request<Permission[]>('/permissions'),

  create: (input: RoleInput) => request<{ id: number }>('/roles', { method: 'POST', body: input }),

  update: (id: number, input: RoleInput) =>
    request<{ ok: boolean }>(`/roles/${id}`, { method: 'PUT', body: input }),

  remove: (id: number) => request<{ ok: boolean }>(`/roles/${id}`, { method: 'DELETE' }),
};

export const fieldsApi = {
  list: () => request<FieldDef[]>('/fields'),

  create: (input: FieldInput) => request<{ id: number }>('/fields', { method: 'POST', body: input }),

  update: (id: number, input: FieldInput) =>
    request<{ ok: boolean }>(`/fields/${id}`, { method: 'PUT', body: input }),

  remove: (id: number) => request<{ ok: boolean }>(`/fields/${id}`, { method: 'DELETE' }),
};

export const recordsApi = {
  list: (keyword: string, page: number, pageSize: number) => {
    const params = new URLSearchParams({
      keyword,
      page: String(page),
      pageSize: String(pageSize),
    });
    return request<RecordPage>(`/records?${params}`);
  },

  get: (id: number) => request<RecordItem>(`/records/${id}`),

  update: (id: number, data: RecordData) =>
    request<{ ok: boolean }>(`/records/${id}`, { method: 'PUT', body: { data } }),

  remove: (id: number) => request<{ ok: boolean }>(`/records/${id}`, { method: 'DELETE' }),

  /** 导出是浏览器直接下载，交给 a 标签处理，不走 fetch */
  exportUrl: (keyword: string) =>
    `/api/records/export?keyword=${encodeURIComponent(keyword)}`,
};

export const importsApi = {
  paste: (text: string) =>
    request<UploadResult>('/imports/paste', { method: 'POST', body: { text } }),

  upload: (file: File) => {
    const form = new FormData();
    form.append('file', file, file.name);
    return requestRaw<UploadResult>('/imports/upload', form);
  },

  preview: (sessionId: string, body: MappingRequest) =>
    request<ImportPreview>(`/imports/${sessionId}/preview`, { method: 'POST', body }),

  commit: (sessionId: string, body: MappingRequest) =>
    request<CommitOutcome>(`/imports/${sessionId}/commit`, { method: 'POST', body }),

  templates: () => request<ImportTemplate[]>('/imports/templates'),

  batches: () => request<ImportBatch[]>('/imports/batches'),
};

export const flowsApi = {
  defs: () => request<FlowDef[]>('/flows/defs'),
  def: (id: number) => request<FlowDef>(`/flows/defs/${id}`),

  createDef: (input: FlowDefInput) =>
    request<{ id: number }>('/flows/defs', { method: 'POST', body: input }),

  updateDef: (id: number, input: FlowDefInput) =>
    request<{ ok: boolean }>(`/flows/defs/${id}`, { method: 'PUT', body: input }),

  removeDef: (id: number) => request<{ ok: boolean }>(`/flows/defs/${id}`, { method: 'DELETE' }),

  start: (input: { defId: number; title: string; recordId?: number | null; form?: RecordData }) =>
    request<{ id: number }>('/flows', { method: 'POST', body: input }),

  instances: (scope: 'created' | 'involved' | 'all') =>
    request<InstanceItem[]>(`/flows/instances?scope=${scope}`),

  instance: (id: number) => request<InstanceDetail>(`/flows/instances/${id}`),

  terminate: (id: number, reason: string) =>
    request<{ ok: boolean }>(`/flows/instances/${id}/terminate`, {
      method: 'POST',
      body: { reason },
    }),

  tasks: (done: boolean) => request<FlowTask[]>(`/flows/tasks?done=${done}`),

  complete: (taskId: number, body: CompleteTaskInput) =>
    request<{ ok: boolean; instanceId: number }>(`/flows/tasks/${taskId}/complete`, {
      method: 'POST',
      body,
    }),
};

export const statsApi = {
  report: (days = 30, overdueHours = 48) =>
    request<StatsReport>(`/stats/report?days=${days}&overdueHours=${overdueHours}`),

  mine: (days = 30, overdueHours = 48) =>
    request<MyStats>(`/stats/mine?days=${days}&overdueHours=${overdueHours}`),
};

export const notificationsApi = {
  list: (unreadOnly = false, limit = 50) =>
    request<NotificationItem[]>(`/notifications?unreadOnly=${unreadOnly}&limit=${limit}`),

  unreadCount: () => request<{ count: number }>('/notifications/unread-count'),

  /** ids 为空表示全部标记为已读 */
  markRead: (ids: number[] = []) =>
    request<{ ok: boolean; affected: number }>('/notifications/read', {
      method: 'POST',
      body: { ids },
    }),

  clear: () =>
    request<{ ok: boolean; affected: number }>('/notifications', { method: 'DELETE' }),
};

export const siteApi = {
  read: () => request<{ name: string }>('/site'),
  update: (name: string) => request<{ name: string }>('/site', { method: 'PUT', body: { name } }),
};
