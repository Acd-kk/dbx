<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import { useConnectionStore } from "@/stores/connectionStore";
import { connectionServerVersion, connectionServerVersionLabel } from "@/lib/connection/connectionServerVersion";

const props = defineProps<{ connectionId: string }>();
const { t } = useI18n();
const connectionStore = useConnectionStore();
const config = computed(() => connectionStore.getConfig(props.connectionId));
const version = computed(() => (config.value ? connectionServerVersion(config.value) : undefined));
const versionLabel = computed(() => (config.value ? connectionServerVersionLabel(config.value) : undefined) ?? t("connection.databaseInfo.unknownVersion"));
const description = computed(() => `${t("connection.databaseInfo.productVersion")}: ${version.value ?? t("connection.databaseInfo.unknownVersion")}`);
</script>

<template>
  <span v-if="config" class="ml-auto min-w-0 max-w-[40%] truncate text-[10px] font-normal leading-4 tabular-nums text-muted-foreground" :title="description" :aria-label="description" data-connection-version>
    {{ versionLabel }}
  </span>
</template>
