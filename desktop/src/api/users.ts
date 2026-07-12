import { request } from "./client";
import type { CreateUserBody, Page, SetRoleBody, User } from "./types";

export async function listUsers(params?: {
  limit?: number;
  cursor?: string;
}): Promise<Page<User>> {
  return request<Page<User>>("/api/v1/admin/users", { query: params });
}

export async function createUser(body: CreateUserBody): Promise<{
  id: string;
  email: string;
  role: string;
}> {
  return request("/api/v1/admin/users", {
    method: "POST",
    body: JSON.stringify(body),
  });
}

export async function setRole(id: string, body: SetRoleBody): Promise<void> {
  await request<void>(`/api/v1/admin/users/${id}/role`, {
    method: "PATCH",
    body: JSON.stringify(body),
  });
}

export async function disableUser(id: string): Promise<void> {
  await request<void>(`/api/v1/admin/users/${id}/disable`, {
    method: "POST",
  });
}

export async function enableUser(id: string): Promise<void> {
  await request<void>(`/api/v1/admin/users/${id}/enable`, {
    method: "POST",
  });
}
