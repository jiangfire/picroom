<template>
  <n-space vertical size="large">
    <div class="teams-header">
      <n-h2>Teams</n-h2>
      <n-button type="primary" @click="showCreateModal = true">Create team</n-button>
    </div>

    <n-data-table
      :columns="columns"
      :data="teams"
      :loading="loading"
      striped
      remote
    />

    <n-modal
      v-model:show="showCreateModal"
      title="Create team"
      preset="card"
      style="width: 440px"
      :segmented="{ content: true, footer: true }"
    >
      <n-form ref="createFormRef" :model="createForm" :rules="createRules">
        <n-form-item label="Name" path="name">
          <n-input v-model:value="createForm.name" placeholder="Engineering" />
        </n-form-item>
        <n-form-item label="Slug" path="slug">
          <n-input v-model:value="createForm.slug" placeholder="engineering" />
        </n-form-item>
        <n-form-item label="Description" path="description">
          <n-input
            v-model:value="createForm.description"
            type="textarea"
            placeholder="Optional"
          />
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

    <n-modal
      v-model:show="showMembersModal"
      title="Team members"
      preset="card"
      style="width: 560px"
    >
      <n-data-table
        :columns="memberColumns"
        :data="members"
        :loading="membersLoading"
        striped
        remote
      />
      <template #footer>
        <n-button @click="showMembersModal = false">Close</n-button>
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
  NSpace,
  useMessage,
  type DataTableColumns,
} from "naive-ui";
import * as teamsApi from "../api/teams";
import type { Team, TeamMember } from "../api/types";

const message = useMessage();
const teams = ref<Team[]>([]);
const loading = ref(false);
const creating = ref(false);
const showCreateModal = ref(false);
const createFormRef = ref<any>(null);
const createForm = ref({
  name: "",
  slug: "",
  description: "",
});

const showMembersModal = ref(false);
const members = ref<TeamMember[]>([]);
const membersLoading = ref(false);
const selectedTeamId = ref<string | null>(null);

const createRules = {
  name: {
    required: true,
    message: "Name is required",
    trigger: ["blur", "input"],
  },
  slug: {
    required: true,
    message: "Slug is required",
    trigger: ["blur", "input"],
  },
};

const columns: DataTableColumns<Team> = [
  { title: "Name", key: "name", ellipsis: { tooltip: true } },
  { title: "Slug", key: "slug", width: 160 },
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
    width: 160,
    render(row) {
      return h(
        NButton,
        { size: "small", onClick: () => viewMembers(row.id) },
        { default: () => "Members" },
      );
    },
  },
];

const memberColumns: DataTableColumns<TeamMember> = [
  { title: "User ID", key: "user_id", ellipsis: { tooltip: true } },
  { title: "Role", key: "role", width: 120 },
  {
    title: "Joined",
    key: "joined_at",
    width: 180,
    render(row) {
      return new Date(row.joined_at).toLocaleString();
    },
  },
];

async function refresh() {
  loading.value = true;
  try {
    const result = await teamsApi.listTeams();
    teams.value = result.items;
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
    await teamsApi.createTeam({
      name: createForm.value.name,
      slug: createForm.value.slug,
      description: createForm.value.description || undefined,
    });
    message.success("Team created");
    showCreateModal.value = false;
    createForm.value = { name: "", slug: "", description: "" };
    await refresh();
  } catch (e) {
    message.error(String(e));
  } finally {
    creating.value = false;
  }
}

async function viewMembers(id: string) {
  selectedTeamId.value = id;
  showMembersModal.value = true;
  membersLoading.value = true;
  try {
    const result = await teamsApi.listTeamMembers(id);
    members.value = result.items;
  } catch (e) {
    message.error(String(e));
  } finally {
    membersLoading.value = false;
  }
}

onMounted(refresh);
</script>

<style scoped>
.teams-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
}
</style>
