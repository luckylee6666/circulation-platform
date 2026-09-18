/** 后端返回的统一错误结构 */
export class ApiError extends Error {
  readonly code: string;
  readonly status: number;

  constructor(code: string, message: string, status: number) {
    super(message);
    this.name = 'ApiError';
    this.code = code;
    this.status = status;
  }
}

type UnauthorizedHandler = () => void;

let unauthorizedHandler: UnauthorizedHandler | null = null;

/** 由鉴权层注入：任何接口返回 401 时调用，用来清掉本地会话状态 */
export function setUnauthorizedHandler(handler: UnauthorizedHandler | null): void {
  unauthorizedHandler = handler;
}

interface RequestOptions {
  method?: 'GET' | 'POST' | 'PUT' | 'DELETE';
  body?: unknown;
  signal?: AbortSignal;
}

export async function request<T>(path: string, options: RequestOptions = {}): Promise<T> {
  const hasBody = options.body !== undefined;

  const response = await fetch(`/api${path}`, {
    method: options.method ?? 'GET',
    credentials: 'same-origin',
    headers: hasBody ? { 'content-type': 'application/json' } : undefined,
    body: hasBody ? JSON.stringify(options.body) : undefined,
    signal: options.signal,
  });

  return handleResponse<T>(response);
}

/** 不走 JSON 序列化的请求（文件上传用 multipart）。 */
export async function requestRaw<T>(path: string, body: BodyInit): Promise<T> {
  const response = await fetch(`/api${path}`, {
    method: 'POST',
    credentials: 'same-origin',
    body,
  });
  return handleResponse<T>(response);
}

async function handleResponse<T>(response: Response): Promise<T> {
  if (response.status === 401) {
    unauthorizedHandler?.();
    throw new ApiError('unauthorized', '登录状态已失效，请重新登录', 401);
  }

  const raw = await response.text();
  const payload = raw ? safeParse(raw) : null;

  if (!response.ok) {
    throw new ApiError(
      readString(payload, 'code') ?? 'error',
      readString(payload, 'message') ?? `请求失败（HTTP ${response.status}）`,
      response.status,
    );
  }

  return payload as T;
}

function safeParse(raw: string): unknown {
  try {
    return JSON.parse(raw);
  } catch {
    return null;
  }
}

function readString(payload: unknown, key: string): string | null {
  if (payload && typeof payload === 'object' && key in payload) {
    const value = (payload as Record<string, unknown>)[key];
    if (typeof value === 'string') return value;
  }
  return null;
}

/** 把各种异常统一成能直接显示给用户的文案 */
export function describeError(error: unknown): string {
  if (error instanceof ApiError) return error.message;
  if (error instanceof Error) return error.message;
  return '操作失败，请稍后重试';
}
