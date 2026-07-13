<template>
  <n-space vertical size="large">
    <div class="storage-header">
      <n-h2>Storage Policies</n-h2>
      <n-button type="primary" @click="showCreateModal = true">Create policy</n-button>
    </div>

    <n-data-table
      :columns="columns"
      :data="policies"
      :loading="loading"
      striped
      remote
    />

    <n-modal
      v-model:show="showCreateModal"
      title="Create storage policy"
      preset="card"
      style="width: 480px"
      :segmented="{ content: true, footer: true }"
    >
      <n-form ref="createFormRef" :model="createForm" :rules="createRules">
        <n-form-item label="Name" path="name">
          <n-input v-model:value="createForm.name" placeholder="s3-main" />
        </n-form-item>
        <n-form-item label="Driver" path="driver">
          <n-select v-model:value="createForm.driver" :options="driverOptions" />
        </n-form-item>
        <n-form-item label="Config JSON" path="config">
          <n-input
            v-model:value="createForm.config"
            type="textarea"
            rows="5"
            placeholder='{}'
          />
        </n-form-item>
        <n-form-item path="is_default">
          <n-checkbox v-model:checked="createForm.is_default">Default policy</n-checkbox>
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
import { onMounted, ref } from "vue";
import {
  NButton,
  NCheckbox,
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
import * as storageApi from "../api/storage";
import type { StoragePolicy } from "../api/types";

const message = useMessage();
const policies = ref<StoragePolicy[]>([]);
const loading = ref(false);
const creating = ref(false);
const showCreateModal = ref(false);
const createFormRef = ref<any>(null);
const createForm = ref({
  name: "",
  driver: "local",
  config: "{}",
  is_default: false,
});

const driverOptions = [
  { label: "Local", value: "local" },
  { label: "S3", value: "s3" },
  { label: "OSS", value: "oss" },
  { label: "COS", value: "cos" },
  { label: "Qiniu", value: "qiniu" },
  { label: "MinIO", value: "minio" },
];

const createRules = {
  name: {
    required: true,
    message: "Name is required",
    trigger: ["blur", "input"],
  },
  driver: {
    required: true,
    message: "Driver is required",
    trigger: ["blur", "input"],
  },
  config: {
    required: true,
    validator(_rule: unknown, value: string) {
      try {
        JSON.parse(value);
        return true;
      } catch {
        return new Error("Invalid JSON");
      }
    },
    trigger: ["blur", "input"],
  },
};

const columns: DataTableColumns<StoragePolicy> = [
  { title: "Name", key: "name", ellipsis: { tooltip: true } },
  { title: "Driver", key: "driver", width: 120 },
  {
    title: "Default",
    key: "is_default",
    width: 100,
    render(row) {
      return row.is_default ? "Yes" : "No";
    },
  },
  {
    title: "Config",
    key: "config",
    ellipsis: { tooltip: true },
    render(row) {
      return JSON.stringify(row.config);
    },
  },
];

async function refresh() {
  loading.value = true;
  try {
    const result = await storageApi.listPolicies();
    policies.value = result.items;
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
    await storageApi.createPolicy({
      name: createForm.value.name,
      driver: createForm.value.driver,
      config: JSON.parse(createForm.value.config),
      is_default: createForm.value.is_default,
    });
    message.success("Policy created");
    showCreateModal.value = false;
    createForm.value = { name: "", driver: "local", config: "{}", is_default: false };
    await refresh();
  } catch (e) {
    message.error(String(e));
  } finally {
    creating.value = false;
  }
}

onMounted(refresh);
</script>

<style scoped>
.storage-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
}
</style>
