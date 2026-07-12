export interface Image {
  id: string;
  content_type: string;
  bytes: number;
  width: number;
  height: number;
  team_id?: string;
  created_at: string;
}

export interface ImageUploadResult {
  id: string;
  bytes: number;
  width: number;
  height: number;
  content_type: string;
  team_id?: string;
  created_at: string;
}

export interface LinkResponse {
  public_url: string;
  expires_at: string | null;
}

export interface Page<T> {
  items: T[];
  has_more: boolean;
  next_cursor?: string;
}

export interface User {
  id: string;
  email: string;
  name: string;
  role: string;
  disabled: boolean;
  created_at: string;
}

export interface CreateUserBody {
  email: string;
  password: string;
  name?: string;
  role?: string;
}

export interface SetRoleBody {
  role: string;
}

export interface Team {
  id: string;
  name: string;
  slug: string;
  description?: string;
  storage_policy?: string;
  created_at: string;
}

export interface CreateTeamBody {
  name: string;
  slug: string;
  description?: string;
}

export interface TeamMember {
  user_id: string;
  role: string;
  joined_at: string;
}

export interface AddMemberBody {
  user_id: string;
  role: string;
}

export interface StoragePolicy {
  name: string;
  driver: string;
  config: Record<string, unknown>;
  is_default: boolean;
}

export interface CreatePolicyBody {
  name: string;
  driver: string;
  config: Record<string, unknown>;
  is_default: boolean;
}

export interface UploadProgress {
  kind: "progress" | "done" | "error";
  bytes_sent?: number;
  total_bytes?: number;
  error?: string;
}

export interface AuditEvent {
  id: string;
  timestamp: string;
  actor_id?: string;
  actor_label?: string;
  action: string;
  target_type: string;
  target_id?: string;
  ip?: string;
  user_agent?: string;
  metadata: unknown;
}
