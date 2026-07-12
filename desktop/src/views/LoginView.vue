<template>
  <div class="login-page">
    <n-card title="Connect to Picroom" class="login-card">
      <n-form
        ref="formRef"
        :model="form"
        :rules="rules"
        label-placement="left"
        label-width="auto"
      >
        <n-form-item label="Server URL" path="server_url">
          <n-input
            v-model:value="form.server_url"
            placeholder="http://localhost:8080"
            @keydown.enter="handleLogin"
          />
        </n-form-item>
        <n-form-item label="Email" path="email">
          <n-input
            v-model:value="form.email"
            placeholder="admin@example.com"
            @keydown.enter="handleLogin"
          />
        </n-form-item>
        <n-form-item label="Password" path="password">
          <n-input
            v-model:value="form.password"
            type="password"
            placeholder="Password"
            show-password-on="mousedown"
            @keydown.enter="handleLogin"
          />
        </n-form-item>
        <n-button
          type="primary"
          :loading="authStore.loading"
          :disabled="authStore.loading"
          @click="handleLogin"
          block
        >
          Connect
        </n-button>
      </n-form>
      <n-alert
        v-if="authStore.error"
        type="error"
        :title="authStore.error"
        style="margin-top: 1rem"
        closable
        @close="authStore.error = null"
      />
    </n-card>
  </div>
</template>

<script setup lang="ts">
import { ref } from "vue";
import { useRouter } from "vue-router";
import {
  NAlert,
  NButton,
  NCard,
  NForm,
  NFormItem,
  NInput,
} from "naive-ui";
import { useAuthStore } from "../stores/auth";

const authStore = useAuthStore();
const router = useRouter();

const formRef = ref<any>(null);
const form = ref({
  server_url: "http://localhost:8080",
  email: "",
  password: "",
});

const rules = {
  server_url: {
    required: true,
    message: "Server URL is required",
    trigger: ["blur", "input"],
  },
  email: {
    required: true,
    message: "Email is required",
    trigger: ["blur", "input"],
  },
  password: {
    required: true,
    message: "Password is required",
    trigger: ["blur", "input"],
  },
};

async function handleLogin() {
  try {
    await formRef.value?.validate();
  } catch {
    return;
  }

  try {
    await authStore.login(form.value);
    await router.replace({ name: "home" });
  } catch {
    // Error is surfaced in authStore.error.
  }
}
</script>

<style scoped>
.login-page {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 100%;
}

.login-card {
  width: 420px;
}
</style>
