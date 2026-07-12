<template>
  <n-space vertical size="large">
    <div class="users-header">
      <n-h2>Users</n-h2>
      <n-button type="primary" @click="showCreateModal = true">Create user</n-button>
    </div>

    <n-data-table
      :columns="columns"
      :data="users"
      :loading="loading"
      striped
      remote
    />

    <n-modal
      v-model:show="showCreateModal"
      title="Create user"
      preset="card"
      style="width: 440px"
      :segmented="{ content: true, footer: true }"
    >
      <n-form ref="createFormRef" :model="createForm" :rules="createRules">
        <n-form-item label="Email" path="email">
          <n-input v-model:value="createForm.email" placeholder="user@example.com" />
        </n-form-item>
        <n-form-item label="Password" path="password">
          <n-input
            v-model:value="createForm.password"
            type="password"
            placeholder="At least 8 characters"
          />
        </n-form-item>
        <n-form-item label="Name" path="name">
          <n-input v-model:value="createForm.name" placeholder="Display name" />
        </n-form-item>
        <n-form-item label="Role" path="role">
          <n-select v-model:value="createForm.role" :options="roleOptions" />
        </n-form-item>
      </n-form>
      <template #footer>
        <n-space justify="end">
          <n-button @click="showCreateModal = false">Cancel</n-button>
          <n-button type="primary" :loading="creating" @click="submitCreate"
            >Create</n-button
          >
        </n-space>
      </template>
    </n-modal>
  </n-space>
</template>

<script setup lang="ts">
import { h, onMounted, ref } from "vue";
import {
  NButton,
  NDataTable,
  NForm,
  NFormItem,
  NH2,
  NInput,
  NModal,
  NSelect,
  NSpace,
  useMessage,
  type DataTableColumns,
} from "naive-ui";
import * as usersApi from "../api/users";
import type { User } from "../api/types";

const message = useMessage();
const users = ref<User[]>([]);
const loading = ref(false);
const creating = ref(false);
const showCreateModal = ref(false);
const createFormRef = ref<any>(null);
const createForm = ref({
  email: "",
  password: "",
  name: "",
  role: "viewer",
});

const roleOptions = [
  { label: "Viewer", value: "viewer" },
  { label: "Uploader", value: "uploader" },
  { label: "Manager", value: "manager" },
  { label: "Admin", value: "admin" },
];

const createRules = {
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
  role: {
    required: true,
    message: "Role is required",
    trigger: ["blur", "input"],
  },
};

const columns: DataTableColumns<User> = [
  { title: "Email", key: "email", ellipsis: { tooltip: true } },
  { title: "Name", key: "name", ellipsis: { tooltip: true } },
  { title: "Role", key: "role", width: 100 },
  {
    title: "Status",
    key: "disabled",
    width: 120,
    render(row) {
      return row.disabled ? "Disabled" : "Active";
    },
  },
  {
    title: "Created",
    key: "created_at",
    width: 180,
    render(row) {
      return new Date(row.created_at).toLocaleString();
    },
  },
  {
    title: "Actions",
    key: "actions",
    width: 260,
    render(row) {
      return h(
        NSpace,
        { size: "small" },
        {
          default: () => [
            h(
              NButton,
              {
                size: "small",
                onClick: () => toggleDisabled(row),
              },
              { default: () => (row.disabled ? "Enable" : "Disable") },
            ),
            h(
              NSelect,
              {
                size: "small",
                value: row.role,
                options: roleOptions,
                style: "width: 120px",
                "onUpdate:value": (v: string) => setRole(row, v),
              },
              undefined,
            ),
          ],
        },
      );
    },
  },
];

async function refresh() {
  loading.value = true;
  try {
    const page = await usersApi.listUsers({ limit: 100 });
    users.value = page.items;
  } catch (e) {
    message.error(String(e));
  } finally {
    loading.value = false;
  }
}

async function submitCreate() {
  try {
    await createFormRef.value?.validate();
  } catch {
    return;
  }
  creating.value = true;
  try {
    await usersApi.createUser({
      email: createForm.value.email,
      password: createForm.value.password,
      name: createForm.value.name || undefined,
      role: createForm.value.role,
    });
    message.success("User created");
    showCreateModal.value = false;
    createForm.value = { email: "", password: "", name: "", role: "viewer" };
    await refresh();
  } catch (e) {
    message.error(String(e));
  } finally {
    creating.value = false;
  }
}

async function toggleDisabled(user: User) {
  try {
    if (user.disabled) {
      await usersApi.enableUser(user.id);
      message.success("User enabled");
    } else {
      await usersApi.disableUser(user.id);
      message.success("User disabled");
    }
    await refresh();
  } catch (e) {
    message.error(String(e));
  }
}

async function setRole(user: User, role: string) {
  try {
    await usersApi.setRole(user.id, { role });
    message.success("Role updated");
    await refresh();
  } catch (e) {
    message.error(String(e));
  }
}

onMounted(refresh);
</script>

<style scoped>
.users-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
}
</style>
