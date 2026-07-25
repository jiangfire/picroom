<template>
  <n-space vertical size="large">
    <div class="images-header">
      <n-h2>Images</n-h2>
      <n-button @click="refresh">Refresh</n-button>
    </div>

    <UploadDropzone @uploaded="refresh" />

    <ImageGrid :images="images" :loading="loading" @deleted="refresh" />
  </n-space>
</template>

<script setup lang="ts">
import { onMounted, ref } from "vue";
import { NH2, NSpace, useMessage } from "naive-ui";
import * as imagesApi from "../api/images";
import type { Image } from "../api/types";
import UploadDropzone from "../components/UploadDropzone.vue";
import ImageGrid from "../components/ImageGrid.vue";

const message = useMessage();
const images = ref<Image[]>([]);
const loading = ref(false);

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

onMounted(refresh);
</script>

<style scoped>
.images-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
}
</style>
