<template>
  <n-space vertical size="large">
    <n-h2>Welcome to Picroom Admin</n-h2>
    <n-p>Choose a section from the sidebar to get started.</n-p>
    <n-grid cols="3" :x-gap="16" :y-gap="16">
      <n-grid-item v-for="card in cards" :key="card.name">
        <n-card hoverable @click="router.push({ name: card.name })">
          <n-h3>{{ card.title }}</n-h3>
          <n-p>{{ card.description }}</n-p>
        </n-card>
      </n-grid-item>
    </n-grid>
  </n-space>
</template>

<script setup lang="ts">
import { useRouter } from "vue-router";
import {
  NCard,
  NGrid,
  NGridItem,
  NH2,
  NH3,
  NP,
  NSpace,
} from "naive-ui";

const router = useRouter();

const cards = [
  {
    name: "images",
    title: "Images",
    description: "Upload, browse, copy links, and manage images.",
  },
  {
    name: "users",
    title: "Users",
    description: "Manage user accounts and roles.",
  },
  {
    name: "teams",
    title: "Teams",
    description: "View teams and team members.",
  },
  {
    name: "storage",
    title: "Storage",
    description: "Configure storage policies.",
  },
  {
    name: "audit",
    title: "Audit",
    description: "Review the audit log.",
  },
  {
    name: "settings",
    title: "Settings",
    description: "Manage server profiles and preferences.",
  },
];
</script>
