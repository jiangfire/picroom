<template>
  <n-data-table
    :columns="columns"
    :data="images"
    :loading="loading"
    :pagination="false"
    remote
    striped
  />
</template>

<script setup lang="ts">
import { h } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import {
  NButton,
  NDataTable,
  NSpace,
  useMessage,
  type DataTableColumns,
} from "naive-ui";
import CopyLinkButton from "./CopyLinkButton.vue";
import * as imagesApi from "../api/images";
import type { Image } from "../api/types";

withDefaults(
  defineProps<{
    images: Image[];
    loading?: boolean;
  }>(),
  { loading: false },
);

const emit = defineEmits<{
  /** Emitted after an image is deleted so the parent can refresh. */
  deleted: [id: string];
}>();

const message = useMessage();

function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${parseFloat((bytes / k ** i).toFixed(2))} ${sizes[i]}`;
}

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
            h(CopyLinkButton, { imageId: row.id }),
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
    await invoke("download_image", { image_id: id, save_path: savePath });
    message.success("Image downloaded");
  } catch (e) {
    message.error(String(e));
  }
}

async function remove(id: string) {
  try {
    await imagesApi.deleteImage(id);
    message.success("Image deleted");
    emit("deleted", id);
  } catch (e) {
    message.error(String(e));
  }
}
</script>
