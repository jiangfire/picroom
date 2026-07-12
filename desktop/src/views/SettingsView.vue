<template>
  <n-space vertical size="large">
    <n-h2>Settings</n-h2>

    <n-card title="Active Profile">
      <n-descriptions bordered>
        <n-descriptions-item label="Name">
          {{ activeProfile?.name ?? "—" }}
        </n-descriptions-item>
        <n-descriptions-item label="Server">
          {{ activeProfile?.server_url ?? "—" }}
        </n-descriptions-item>
        <n-descriptions-item label="Email">
          {{ activeProfile?.email ?? "—" }}
        </n-descriptions-item>
      </n-descriptions>
      <n-button
        type="error"
        style="margin-top: 1rem"
        :loading="authStore.loading"
        @click="handleLogout"
      >
        Logout
      </n-button>
    </n-card>

    <n-card title="Saved Profiles">
      <n-list bordered>
        <n-list-item v-for="profile in profiles" :key="profile.name">
          <template #prefix>
            <n-tag v-if="profile.name === activeProfile?.name" type="success"
              >Active</n-tag
            >
          </template>
          <n-thing
            :title="profile.name"
            :description="`${profile.email} @ ${profile.server_url}`"
          />
          <template #suffix>
            <n-space>
              <n-button
                size="small"
                :disabled="profile.name === activeProfile?.name"
                @click="activateProfile(profile.name)"
              >
                Activate
              </n-button>
              <n-button
                size="small"
                type="error"
                @click="deleteProfile(profile.name)"
              >
                Delete
              </n-button>
            </n-space>
          </template>
        </n-list-item>
      </n-list>
    </n-card>
  </n-space>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useRouter } from "vue-router";
import {
  NButton,
  NCard,
  NDescriptions,
  NDescriptionsItem,
  NH2,
  NList,
  NListItem,
  NSpace,
  NTag,
  NThing,
  useMessage,
} from "naive-ui";
import { useAuthStore, type Profile } from "../stores/auth";

const authStore = useAuthStore();
const router = useRouter();
const message = useMessage();

const activeProfile = computed(() => authStore.session);
const profiles = ref<Profile[]>([]);

async function loadProfiles() {
  try {
    profiles.value = await authStore.listProfiles();
  } catch (e) {
    message.error(String(e));
  }
}

async function activateProfile(name: string) {
  try {
    await authStore.setActiveProfile(name);
    await authStore.restoreSession();
    message.success("Profile activated");
  } catch (e) {
    message.error(String(e));
  }
}

async function deleteProfile(name: string) {
  try {
    await authStore.removeProfile(name);
    await loadProfiles();
    if (name === activeProfile.value?.name) {
      await authStore.restoreSession();
    }
    message.success("Profile deleted");
  } catch (e) {
    message.error(String(e));
  }
}

async function handleLogout() {
  await authStore.logout();
  await router.replace({ name: "login" });
  message.success("Logged out");
}

onMounted(loadProfiles);
</script>
