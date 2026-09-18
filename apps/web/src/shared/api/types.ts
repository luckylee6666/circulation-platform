/** 与 Rust 侧 `#[serde(rename_all = "camelCase")]` 结构一一对应 */

export interface RoleBrief {
  id: number;
  code: string;
  name: string;
}

export interface User {
  id: number;
  username: string;
  displayName: string;
  phone: string;
  dept: string;
  email: string;
  status: number;
  mustChangePassword: boolean;
  lastLoginAt: string | null;
  createdAt: string;
  roles: RoleBrief[];
}

export interface SessionInfo {
  user: User;
  roles: string[];
  permissions: string[];
  mustChangePassword: boolean;
}

export interface Role {
  id: number;
  code: string;
  name: string;
  description: string;
  isSystem: boolean;
  userCount: number;
  permissions: string[];
}

export interface Permission {
  code: string;
  name: string;
  category: string;
}

export interface UserOption {
  id: number;
  displayName: string;
  dept: string;
}

export interface UserInput {
  username: string;
  displayName: string;
  password?: string;
  phone?: string;
  dept?: string;
  email?: string;
  roleIds: number[];
}

export interface RoleInput {
  code: string;
  name: string;
  description: string;
  permissions: string[];
}

export interface CreateUserResult {
  id: number;
  initialPassword: string;
}

// ---------- 字段定义 ----------

export type FieldType = 'text' | 'number' | 'date' | 'select' | 'bool';

export interface FieldDef {
  id: number;
  code: string;
  label: string;
  fieldType: FieldType;
  required: boolean;
  isUniqueKey: boolean;
  options: string[];
  sort: number;
  showInList: boolean;
  searchable: boolean;
  enabled: boolean;
  createdAt: string;
}

export interface FieldInput {
  code: string;
  label: string;
  fieldType: FieldType;
  required: boolean;
  isUniqueKey: boolean;
  options: string[];
  sort: number;
  showInList: boolean;
  searchable: boolean;
  enabled: boolean;
}

// ---------- 记录 ----------

export type RecordData = Record<string, unknown>;

export interface RecordItem {
  id: number;
  extKey: string;
  data: RecordData;
  createdAt: string;
  updatedAt: string;
}

export interface RecordPage {
  total: number;
  page: number;
  pageSize: number;
  items: RecordItem[];
}

// ---------- 导入 ----------

export interface ImportTemplate {
  id: number;
  name: string;
  headerFingerprint: string;
  mapping: Record<string, number>;
  dedupe: DedupeStrategy;
  updatedAt: string;
}

export type DedupeStrategy = 'upsert' | 'skip' | 'insert';

export interface UploadResult {
  sessionId: string;
  sourceName: string;
  totalRows: number;
  headerRow: number;
  previewRows: string[][];
  suggestedMapping: Record<string, number>;
  matchedTemplate: ImportTemplate | null;
  uniqueKeyLabel: string | null;
}

export type RowStatus = 'insert' | 'update' | 'skip' | 'invalid';

export interface PreviewRow {
  rowNumber: number;
  status: RowStatus;
  errors: string[];
  data: RecordData;
  extKey: string;
}

export interface PreviewSummary {
  total: number;
  insert: number;
  update: number;
  skip: number;
  invalid: number;
}

export interface ImportPreview {
  summary: PreviewSummary;
  rows: PreviewRow[];
  mappingErrors: string[];
}

export interface CommitOutcome {
  batchId: number;
  total: number;
  inserted: number;
  updated: number;
  skipped: number;
  failed: number;
  errors: PreviewRow[];
}

export interface ImportBatch {
  id: number;
  filename: string;
  totalRows: number;
  inserted: number;
  updated: number;
  skipped: number;
  failed: number;
  operatorId: number | null;
  operatorName: string;
  createdAt: string;
}

export interface MappingRequest {
  headerRow: number;
  mapping: Record<string, number>;
  dedupe: DedupeStrategy;
  templateName?: string;
}

// ---------- 流转 ----------

export type StepKind = 'start' | 'dispatch' | 'handle' | 'confirm';
export type AssignMode = 'manual' | 'role';
export type CompleteRule = 'all' | 'any';

export type Assignee =
  | { mode: 'role'; roles: string[] }
  | { mode: 'users'; userIds: number[] }
  | { mode: 'initiator' }
  | { mode: 'assigned' };

export interface StepConfig {
  key: string;
  name: string;
  kind: StepKind;
  assignee: Assignee;
  /** 留空表示按顺序进入下一个步骤 */
  next?: string | null;
  /** confirm 打回到哪一步 */
  onReject?: string | null;
  completeRule: CompleteRule;
  assignMode: AssignMode;
}

export type ConditionOp = 'eq' | 'ne' | 'gt' | 'lt' | 'contains' | 'empty' | 'notEmpty';

export interface Condition {
  field: string;
  op: ConditionOp;
  value: unknown;
}

export interface RoutingRule {
  onStep: string;
  when: Condition;
  goto: string;
}

export interface FlowConfig {
  steps: StepConfig[];
  rules: RoutingRule[];
}

export interface FlowDef {
  id: number;
  code: string;
  name: string;
  description: string;
  version: number;
  enabled: boolean;
  isDefault: boolean;
  config: FlowConfig;
  createdAt: string;
  updatedAt: string;
}

export interface FlowDefInput {
  code: string;
  name: string;
  description: string;
  config: FlowConfig;
  enabled: boolean;
  isDefault: boolean;
}

export interface FlowTask {
  id: number;
  instanceId: number;
  instanceCode: string;
  title: string;
  stepKey: string;
  stepName: string;
  stepType: string;
  status: number;
  assigneeId: number;
  assigneeName: string;
  action: string;
  comment: string;
  createdAt: string;
  doneAt: string | null;
  initiatorName: string;
}

export interface InstanceItem {
  id: number;
  code: string;
  title: string;
  defName: string;
  status: number;
  currentStepKey: string;
  currentStepName: string;
  initiatorId: number;
  initiatorName: string;
  createdAt: string;
  updatedAt: string;
  finishedAt: string | null;
  pendingCount: number;
}

export interface FlowLog {
  id: number;
  actorName: string;
  action: string;
  fromStep: string;
  toStep: string;
  detail: string;
  createdAt: string;
}

export interface InstanceDetail {
  id: number;
  code: string;
  defId: number;
  defName: string;
  title: string;
  status: number;
  currentStepKey: string;
  currentStepName: string;
  currentStepType: string;
  initiatorId: number;
  initiatorName: string;
  recordId: number | null;
  form: RecordData;
  createdAt: string;
  updatedAt: string;
  finishedAt: string | null;
  tasks: FlowTask[];
  logs: FlowLog[];
  myPendingTaskId: number | null;
}

export interface CompleteTaskInput {
  /** complete / pass / reject */
  action: 'complete' | 'pass' | 'reject';
  comment?: string;
  assignedTo?: number[];
}

// ---------- 站内消息 ----------

export interface NotificationItem {
  id: number;
  /** taskAssigned / taskRejected / flowFinished / flowTerminated */
  category: string;
  title: string;
  content: string;
  refType: string;
  refId: number | null;
  readAt: string | null;
  createdAt: string;
}

// ---------- 统计 ----------

export interface StatsOverview {
  total: number;
  running: number;
  finished: number;
  terminated: number;
  createdToday: number;
  finishedToday: number;
  overdue: number;
  overdueHours: number;
  avgFinishHours: number | null;
  avgRunningHours: number | null;
}

export interface StepStat {
  stepKey: string;
  stepName: string;
  stepType: string;
  total: number;
  pending: number;
  done: number;
  avgHours: number | null;
}

export interface AssigneeStat {
  userId: number;
  displayName: string;
  dept: string;
  pending: number;
  done: number;
  overdue: number;
  avgHours: number | null;
}

export interface TrendPoint {
  day: string;
  created: number;
  finished: number;
}

export interface DataSummary {
  records: number;
  batches: number;
  lastImportAt: string | null;
  lastImportRows: number;
}

export interface StatsReport {
  overview: StatsOverview;
  steps: StepStat[];
  assignees: AssigneeStat[];
  trend: TrendPoint[];
  data: DataSummary;
}

export interface MyStats {
  pending: number;
  done: number;
  created: number;
  overdue: number;
  overdueHours: number;
  avgHours: number | null;
  trend: TrendPoint[];
}
