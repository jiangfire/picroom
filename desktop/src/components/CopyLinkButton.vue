<template>
  <n-button size="small" :loading="loading" @click="copy">{{ label }}</n-button>
</template>

<script setup lang="ts">
import { ref } from "vue";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { useMessage } from "naive-ui";
import * as imagesApi from "../api/images";
import { useAuthStore } from "../stores/auth";

const props = withDefaults(
  defineProps<{
    /** Image id whose public link should be copied. */
    imageId: string;
    /** Button caption. */
    label?: string;
  }>(),
  { label: "Copy link" },
);

const authStore = useAuthStore();
const message = useMessage();
const loading = ref(false);

async function copy() {
  loading.value = true;
  try {
    const link = await imagesApi.getImageLink(props.imageId);
    const url = link.public_url.startsWith("http")
      ? link.public_url
      : `${authStore.session?.server_url ?? ""}${link.public_url}`;
    await writeText(url);
    message.success("Public link copied to clipboard");
  } catch (e) {
    message.error(String(e));
  } finally {
    loading.value = false;
  }
}
</script>
