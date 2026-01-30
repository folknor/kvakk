<script setup lang="ts">
import { PropType } from 'vue';
import { TauriVM } from '../vue_lib/helper/ParamsHelper';

defineProps({
	vm: {
		type: Object as PropType<TauriVM>,
		required: true
	}
});
</script>

<template>
	<div class="flex flex-row justify-between items-center px-6 py-4">
		<!-- Header, Pc name left and version right -->
		<div>
			<h4 class="text-md">
				Device name
			</h4>
			<h2 class="text-2xl font-medium">
				{{ vm.hostname }}
			</h2>
		</div>
		<div class="flex justify-center items-center gap-4">
			<div class="flex items-center gap-2 text-sm transition duration-150 ease-in-out">
				<p>
					v{{ vm.version }}
				</p>
				<p v-if="vm.new_version" class="text-green-600 font-medium">
					(v{{ vm.new_version }} available)
				</p>
			</div>
		</div>
	</div>
</template>
