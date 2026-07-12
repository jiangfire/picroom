<template>
  <n-space vertical size="large">
    <div class="images-header">
      <n-h2>Images</n-h2>
      <div class="images-actions">
        <n-button @click="refresh">Refresh</n-button>
        <n-button type="primary" @click="upload">Upload</n-button>
      </div>
    </div>

    <div
      class="drop-zone"
      :class="{ 'drop-zone-active': dragOver }"
      @dragenter.prevent
      @dragover.prevent
    >
      <p v-if="dragOver">Drop images here to upload</p>
      <p v-else>Drag and drop images here, or use the Upload button</p>
    </div>

    <n-data-table
      :columns="columns"
      :data="images"
      :loading="loading"
      :pagination="false"
      remote
      striped
    />
  </n-space>
</template>

<script setup lang="ts">
import { h, onMounted, onUnmounted, ref } from "vue";
import { Channel, invoke } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open, save } from "@tauri-apps/plugin-dialog";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import {
  NButton,
  NDataTable,
  NH2,
  NSpace,
  useMessage,
  type DataTableColumns,
} from "naive-ui";
import * as imagesApi from "../api/images";
import { useAuthStore } from "../stores/auth";
import type { Image, LinkResponse, UploadProgress } from "../api/types";

const authStore = useAuthStore();
const message = useMessage();

const images = ref<Image[]>([]);
const loading = ref(false);
const dragOver = ref(false);
let unlistenDragDrop: (() => void) | null = null;

const columns: DataTableColumns<Image> = [
  { title: "ID", key: "id", ellipsis: { tooltip: true } },
  { title: "Type", key: "content_type", width: 120 },
  {
    title: "Size",
    key: "bytes",
    width: 100,
    render(row) {
      return formatBytes(row.bytes);
    },
  },
  {
    title: "Dimensions",
    key: "dimensions",
    width: 120,
    render(row) {
      return `${row.width} × ${row.height}`;
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
    width: 300,
    render(row) {
      return h(
        NSpace,
        { size: "small" },
        {
          default: () => [
            h(
              NButton,
              { size: "small", onClick: () => copyLink(row.id) },
              { default: () => "Copy link" },
            ),
            h(
              NButton,
              { size: "small", onClick: () => download(row.id) },
              { default: () => "Download" },
            ),
            h(
              NButton,
              { size: "small", type: "error", onClick: () => remove(row.id) },
              { default: () => "Delete" },
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
    const page = await imagesApi.listImages({ limit: 50 });
    images.value = page.items;
  } catch (e) {
    message.error(String(e));
  } finally {
    loading.value = false;
  }
}

async function copyLink(id: string) {
  try {
    const link: LinkResponse = await imagesApi.getImageLink(id);
    const url = link.public_url.startsWith("http")
      ? link.public_url
      : `${authStore.session?.server_url ?? ""}${link.public_url}`;
    await writeText(url);
    message.success("Public link copied to clipboard");
  } catch (e) {
    message.error(String(e));
  }
}

async function download(id: string) {
  const savePath = await save({
    defaultPath: `image-${id}.bin`,
    filters: [
      {
        name: "Image",
        extensions: ["jpg", "jpeg", "png", "webp", "avif", "gif"],
      },
    ],
  });
  if (!savePath) {
    return;
  }
  try {
    await invoke("download_image", { imageId: id, savePath });
    message.success("Image downloaded");
  } catch (e) {
    message.error(String(e));
  }
}

async function remove(id: string) {
  try {
    await imagesApi.deleteImage(id);
    message.success("Image deleted");
    await refresh();
  } catch (e) {
    message.error(String(e));
  }
}

async function upload() {
  const selected = await open({
    multiple: false,
    directory: false,
    filters: [
      {
        name: "Images",
        extensions: ["jpg", "jpeg", "png", "webp", "avif", "gif"],
      },
    ],
  });
  if (!selected || Array.isArray(selected)) {
    return;
  }
  await uploadFile(selected);
}

async function uploadFile(path: string) {
  const channel = new Channel<UploadProgress>();
  channel.onmessage = (progress) => {
    if (progress.kind === "done") {
      message.success("Upload complete");
    } else if (progress.kind === "error") {
      message.error(progress.error ?? "Upload failed");
    }
  };

  try {
    await invoke("upload_file", {
      filePath: path,
      teamId: undefined,
      onProgress: channel,
    });
    await refresh();
  } catch (e) {
    message.error(String(e));
  }
}

function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${parseFloat((bytes / k ** i).toFixed(2))} ${sizes[i]}`;
}

onMounted(async () => {
  await refresh();
  const webview = getCurrentWebview();
  unlistenDragDrop = await webview.onDragDropEvent((event) => {
    const payload = event.payload;
    if (payload.type === "over") {
      dragOver.value = true;
    } else if (payload.type === "drop") {
      dragOver.value = false;
      const paths = (payload as { paths: string[] }).paths;
      for (const path of paths) {
        void uploadFile(path);
      }
    } else {
      dragOver.value = false;
    }
  });
});

onUnmounted(() => {
  unlistenDragDrop?.();
});
</script>

<style scoped>
.images-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.images-actions {
  display: flex;
  gap: 0.75rem;
}

.drop-zone {
  border: 2px dashed #d1d5db;
  border-radius: 0.5rem;
  padding: 2rem;
  text-align: center;
  color: #6b7280;
  transition: all 0.2s ease;
}

.drop-zone-active {
  border-color: #18a058;
  background-color: #f0fdf4;
  color: #18a058;
}

.drop-zone p {
  margin: 0;
}
</style>
