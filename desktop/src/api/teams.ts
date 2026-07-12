import { request } from "./client";
import type {
  AddMemberBody,
  CreateTeamBody,
  Team,
  TeamMember,
} from "./types";

export async function listTeams(): Promise<{ items: Team[] }> {
  return request<{ items: Team[] }>("/api/v1/teams");
}

export async function createTeam(body: CreateTeamBody): Promise<{
  id: string;
  slug: string;
}> {
  return request("/api/v1/teams", {
    method: "POST",
    body: JSON.stringify(body),
  });
}

export async function getTeam(id: string): Promise<Team> {
  return request<Team>(`/api/v1/teams/${id}`);
}

export async function listTeamMembers(id: string): Promise<{
  items: TeamMember[];
}> {
  return request<{ items: TeamMember[] }>(`/api/v1/teams/${id}/members`);
}

export async function addTeamMember(id: string, body: AddMemberBody): Promise<void> {
  await request<void>(`/api/v1/teams/${id}/members`, {
    method: "POST",
    body: JSON.stringify(body),
  });
}
