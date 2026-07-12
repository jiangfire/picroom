import { request } from "./client";
import type { CreatePolicyBody, StoragePolicy } from "./types";

export async function listPolicies(): Promise<{ items: StoragePolicy[] }> {
  return request<{ items: StoragePolicy[] }>("/api/v1/admin/storage/policies");
}

export async function createPolicy(body: CreatePolicyBody): Promise<{
  name: string;
  driver: string;
  is_default: boolean;
}> {
  return request("/api/v1/admin/storage/policies", {
    method: "POST",
    body: JSON.stringify(body),
  });
}
