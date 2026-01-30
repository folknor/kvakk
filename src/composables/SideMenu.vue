<script setup lang="ts">
import { TauriVM } from '../vue_lib/helper/ParamsHelper';
import { PropType } from 'vue';

const props = defineProps({
	vm: {
		type: Object as PropType<TauriVM>,
		required: true
	}
});

const emits = defineEmits(['clearSending']);

const pluralize = (n: number, s: string) => n === 1 ? s : `${s}s`;
</script>

<template>
	<div class="w-72 p-3" v-if="props.vm.outboundPayload === undefined">
		<p class="mt-4 mb-2 pt-3 px-3">
			Status
		</p>
		<div class="btn font-medium flex flex-row !justify-between w-full items-center rounded-xl p-3 cursor-default">
			<span class="text-green-600">Always visible</span>
			<svg xmlns="http://www.w3.org/2000/svg" height="24" viewBox="0 -960 960 960" width="24" class="text-green-600">
				<path fill="currentColor" d="M480-320q75 0 127.5-52.5T660-500q0-75-52.5-127.5T480-680q-75 0-127.5 52.5T300-500q0 75 52.5 127.5T480-320Zm0-72q-45 0-76.5-31.5T372-500q0-45 31.5-76.5T480-608q45 0 76.5 31.5T588-500q0 45-31.5 76.5T480-392Zm0 192q-146 0-266-81.5T40-500q54-137 174-218.5T480-800q146 0 266 81.5T920-500q-54 137-174 218.5T480-200Z"/>
			</svg>
		</div>
		<p class="text-xs mt-2 pb-3 px-3">
			Nearby devices can share files with you, but you'll always be
			notified and have to approve each transfer before receiving it.
		</p>
	</div>
	<div class="w-72 p-6 flex flex-col justify-between" v-else>
		<div>
			<p class="mt-4 mb-2">
				Sharing {{ props.vm.outboundPayload.Files.length }} {{ pluralize(props.vm.outboundPayload.Files.length, "file") }}
			</p>
			<div class="bg-white w-32 h-32 rounded-2xl mb-2 flex justify-center items-center">
				<svg
					xmlns="http://www.w3.org/2000/svg" height="24" viewBox="0 -960 960 960" width="24"
					class="w-8 h-8">
					<!-- eslint-disable-next-line -->
                    <path d="M240-80q-33 0-56.5-23.5T160-160v-640q0-33 23.5-56.5T240-880h320l240 240v480q0 33-23.5 56.5T720-80H240Zm280-520v-200H240v640h480v-440H520ZM240-800v200-200 640-640Z" />
				</svg>
			</div>
			<p v-for="f in props.vm.outboundPayload.Files" :key="f" class="overflow-hidden whitespace-nowrap text-ellipsis">
				{{ f.split('/').pop() }}
			</p>

			<p class="text-xs mt-3">
				Make sure both devices are unlocked, close together, and have bluetooth turned on. Device you're sharing with need
				Quick Share turned on and visible to you.
			</p>
		</div>

		<p
			@click="emits('clearSending')"
			class="btn px-3 rounded-xl active:scale-95 transition duration-150 ease-in-out w-fit">
			Cancel
		</p>
	</div>
</template>
