import { request } from "./client";
import type { Image, LinkResponse, Page } from "./types";

export async function listImages(params?: {
  owner?: string;
  limit?: number;
  cursor?: string;
}): Promise<Page<Image>> {
  return request<Page<Image>>("/api/v1/images", { query: params });
}

export async function getImage(id: string): Promise<Image> {
  return request<Image>(`/api/v1/images/${id}`);
}

export async function getImageLink(id: string): Promise<LinkResponse> {
  return request<LinkResponse>(`/api/v1/images/${id}/link`);
}

export async function deleteImage(id: string): Promise<void> {
  await request<void>(`/api/v1/images/${id}`, { method: "DELETE" });
}
