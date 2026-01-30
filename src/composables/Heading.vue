<script setup lang="ts">
import type { PropType } from "vue";
import type { TauriVM } from "../vue_lib/helper/ParamsHelper";

defineProps({
    vm: {
        type: Object as PropType<TauriVM>,
        required: true,
    },
});
</script>

<template>
	<div class="flex flex-row justify-between items-center px-6 py-3">
		<h2 class="text-lg font-medium">
			{{ vm.hostname }}
		</h2>
		<svg xmlns="http://www.w3.org/2000/svg" height="20" viewBox="0 -960 960 960" width="20"
			:class="vm.backendReady ? 'text-green-600' : 'text-red-500'"
			:title="vm.backendReady ? 'Ready' : 'Starting...'">
			<path fill="currentColor" d="M480-320q75 0 127.5-52.5T660-500q0-75-52.5-127.5T480-680q-75 0-127.5 52.5T300-500q0 75 52.5 127.5T480-320Zm0-72q-45 0-76.5-31.5T372-500q0-45 31.5-76.5T480-608q45 0 76.5 31.5T588-500q0 45-31.5 76.5T480-392Zm0 192q-146 0-266-81.5T40-500q54-137 174-218.5T480-800q146 0 266 81.5T920-500q-54 137-174 218.5T480-200Z"/>
		</svg>
	</div>
</template>
