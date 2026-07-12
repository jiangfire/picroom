<template>
  <n-config-provider>
    <n-message-provider>
      <n-dialog-provider>
        <div class="app-layout">
          <nav class="app-nav">
            <h1>Picroom Admin</h1>
            <div class="app-nav-spacer" />
            <n-text v-if="authStore.session" type="success">
              {{ authStore.session.email }} @ {{ authStore.session.server_url }}
            </n-text>
            <n-button
              v-if="authStore.isAuthenticated"
              size="small"
              @click="handleLogout"
            >
              Logout
            </n-button>
          </nav>
          <div class="app-body">
            <aside v-if="authStore.isAuthenticated" class="app-sidebar">
              <n-menu
                :value="activeMenuKey"
                :options="menuOptions"
                @update:value="onMenuSelect"
              />
            </aside>
            <main class="app-main">
              <router-view />
            </main>
          </div>
        </div>
      </n-dialog-provider>
    </n-message-provider>
  </n-config-provider>
</template>

<script setup lang="ts">
import { computed } from "vue";
import { useRoute, useRouter } from "vue-router";
import type { MenuOption } from "naive-ui";
import {
  NButton,
  NConfigProvider,
  NDialogProvider,
  NMenu,
  NMessageProvider,
  NText,
} from "naive-ui";
import { useAuthStore } from "./stores/auth";

const authStore = useAuthStore();
const router = useRouter();
const route = useRoute();

const activeMenuKey = computed(() => (route.name as string) ?? "home");

const menuOptions: MenuOption[] = [
  { label: "Home", key: "home" },
  { label: "Images", key: "images" },
  { label: "Users", key: "users" },
  { label: "Teams", key: "teams" },
  { label: "Storage", key: "storage" },
  { label: "Audit", key: "audit" },
  { label: "Settings", key: "settings" },
];

function onMenuSelect(key: string) {
  router.push({ name: key });
}

async function handleLogout() {
  await authStore.logout();
  await router.replace({ name: "login" });
}
</script>

<style>
html,
body,
#app {
  margin: 0;
  padding: 0;
  height: 100%;
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Oxygen,
    Ubuntu, sans-serif;
}

.app-layout {
  display: flex;
  flex-direction: column;
  height: 100%;
}

.app-nav {
  padding: 0.75rem 1.25rem;
  border-bottom: 1px solid #e5e7eb;
  display: flex;
  align-items: center;
  gap: 1rem;
}

.app-nav h1 {
  margin: 0;
  font-size: 1.25rem;
}

.app-nav-spacer {
  flex: 1;
}

.app-body {
  display: flex;
  flex: 1;
  overflow: hidden;
}

.app-sidebar {
  width: 200px;
  border-right: 1px solid #e5e7eb;
  padding: 0.75rem 0;
  overflow-y: auto;
}

.app-main {
  flex: 1;
  overflow: auto;
  padding: 1.25rem;
}
</style>
