<template>
  <n-space vertical size="large">
    <n-h2>Audit Log</n-h2>
    <n-data-table
      :columns="columns"
      :data="events"
      :loading="loading"
      striped
      remote
    />
  </n-space>
</template>

<script setup lang="ts">
import { onMounted, ref } from "vue";
import {
  NDataTable,
  NH2,
  NSpace,
  useMessage,
  type DataTableColumns,
} from "naive-ui";
import * as auditApi from "../api/audit";
import type { AuditEvent } from "../api/types";

const message = useMessage();
const events = ref<AuditEvent[]>([]);
const loading = ref(false);

const columns: DataTableColumns<AuditEvent> = [
  {
    title: "Timestamp",
    key: "timestamp",
    width: 180,
    render(row) {
      return new Date(row.timestamp).toLocaleString();
    },
  },
  { title: "Action", key: "action", width: 160 },
  { title: "Actor", key: "actor_id", ellipsis: { tooltip: true } },
  { title: "Target Type", key: "target_type", width: 120 },
  { title: "Target ID", key: "target_id", ellipsis: { tooltip: true } },
  {
    title: "Metadata",
    key: "metadata",
    ellipsis: { tooltip: true },
    render(row) {
      return JSON.stringify(row.metadata);
    },
  },
];

async function refresh() {
  loading.value = true;
  try {
    events.value = await auditApi.listAudit({ limit: 100 });
  } catch (e) {
    message.error(String(e));
  } finally {
    loading.value = false;
  }
}

onMounted(refresh);
</script>
