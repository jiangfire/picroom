import { defineStore } from "pinia";
import { computed, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";

export interface LoginPayload {
  server_url: string;
  email: string;
  password: string;
}

export interface Session {
  name: string;
  server_url: string;
  email: string;
  token: string;
}

export interface Profile {
  name: string;
  server_url: string;
  email: string;
  token?: string;
}

export const useAuthStore = defineStore("auth", () => {
  const session = ref<Session | null>(null);
  const loading = ref(false);
  const error = ref<string | null>(null);

  const isAuthenticated = computed(() => !!session.value?.token);

  async function restoreSession(): Promise<void> {
    loading.value = true;
    try {
      session.value = await invoke<Session | null>("get_session");
    } finally {
      loading.value = false;
    }
  }

  async function login(payload: LoginPayload): Promise<Session> {
    loading.value = true;
    error.value = null;
    try {
      const result = await invoke<Session>("login", { payload });
      session.value = result;
      return result;
    } catch (e) {
      error.value = String(e);
      throw e;
    } finally {
      loading.value = false;
    }
  }

  async function logout(): Promise<void> {
    await invoke("logout");
    session.value = null;
  }

  async function listProfiles(): Promise<Profile[]> {
    return await invoke<Profile[]>("list_profiles");
  }

  async function saveProfile(profile: Profile): Promise<void> {
    await invoke("save_profile", { profile });
  }

  async function setActiveProfile(name: string): Promise<void> {
    await invoke("set_active_profile", { name });
  }

  async function removeProfile(name: string): Promise<void> {
    await invoke("remove_profile", { name });
  }

  return {
    session,
    loading,
    error,
    isAuthenticated,
    restoreSession,
    login,
    logout,
    listProfiles,
    saveProfile,
    setActiveProfile,
    removeProfile,
  };
});
