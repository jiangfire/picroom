import { request } from "./client";
import type { AuditEvent } from "./types";

export async function listAudit(params?: {
  limit?: number;
  before?: string;
}): Promise<AuditEvent[]> {
  return request<AuditEvent[]>("/api/v1/audit", { query: params });
}
