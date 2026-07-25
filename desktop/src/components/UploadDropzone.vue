<template>
  <div
    class="drop-zone"
    :class="{ 'drop-zone-active': dragOver }"
    @dragenter.prevent
    @dragover.prevent
  >
    <p v-if="dragOver">Drop images here to upload</p>
    <p v-else>Drag and drop images here, or use the Upload button</p>
    <n-button class="upload-btn" type="primary" @click="pick">Upload</n-button>
  </div>
</template>

<script setup lang="ts">
import { onMounted, onUnmounted, ref } from "vue";
import { Channel, invoke } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";
import { useMessage } from "naive-ui";
import type { UploadProgress } from "../api/types";

const emit = defineEmits<{
  /** Emitted after a successful upload so the parent can refresh the list. */
  uploaded: [];
}>();

const message = useMessage();
const dragOver = ref(false);
let unlistenDragDrop: (() => void) | null = null;

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
      file_path: path,
      team_id: undefined,
      on_progress: channel,
    });
    emit("uploaded");
  } catch (e) {
    message.error(String(e));
  }
}

async function pick() {
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

onMounted(async () => {
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

.upload-btn {
  margin-top: 0.75rem;
}
</style>
