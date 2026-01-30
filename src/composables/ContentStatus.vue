<script setup lang="ts">
import type { PropType } from "vue";
import type { OutboundPayload } from "@/bindings/OutboundPayload";
import type { TauriVM } from "../vue_lib/helper/ParamsHelper";

const props: { vm: TauriVM } = defineProps({
    vm: {
        type: Object as PropType<TauriVM>,
        required: true,
    },
});

const emits: {
    (e: "outboundPayload", payload: OutboundPayload): void;
    (e: "discoveryRunning"): void;
} = defineEmits(["outboundPayload", "discoveryRunning"]);

async function openFilePicker(): Promise<void> {
    const el = await props.vm.dialogOpen({
        title: "Select a file to send",
        directory: false,
        multiple: true,
    });

    if (el === null) {
        return;
    }

    let elem: string[];
    if (Array.isArray(el)) {
        if (
            el.length > 0 &&
            typeof el[0] === "object" &&
            el[0] !== null &&
            Object.hasOwn(el[0], "path")
        ) {
            elem = el.map((e) => (e as { path: string }).path);
        } else {
            elem = el as string[];
        }
    } else {
        elem = [el as string];
    }

    emits("outboundPayload", {
        Files: elem,
    } as OutboundPayload);
    if (!props.vm.discoveryRunning) {
        await props.vm.invoke("start_discovery");
    }
    emits("discoveryRunning");
}
</script>

<template>
	<h3 class="mb-4 font-medium text-xl">
		<span v-if="props.vm.displayedIsEmpty">Ready to receive{{ props.vm.outboundPayload !== undefined ? ' / send' : '' }}</span>
		<span v-else>Nearby devices</span>
	</h3>

	<div
		v-if="props.vm.displayedIsEmpty && props.vm.outboundPayload === undefined" class="w-full border
        rounded-2xl p-6 flex flex-col justify-center items-center transition duration-150 ease-in-out mt-auto"
		:class="{'border-green-200 bg-green-100 scale-105': props.vm.isDragHovering}">
		<svg
			xmlns="http://www.w3.org/2000/svg" height="24"
			viewBox="0 -960 960 960" width="24" class="w-8 h-8">
			<!-- eslint-disable-next-line -->
            <path d="M440-320v-326L336-542l-56-58 200-200 200 200-56 58-104-104v326h-80ZM240-160q-33 0-56.5-23.5T160-240v-120h80v120h480v-120h80v120q0 33-23.5 56.5T720-160H240Z" />
		</svg>
		<h4 class="mt-2 font-medium">
			Drop files to send
		</h4>
		<div class="btn mt-2 active:scale-95 transition duration-150 ease-in-out" @click="openFilePicker()">
			<svg xmlns="http://www.w3.org/2000/svg" height="24" viewBox="0 -960 960 960" width="24">
				<path d="M440-440H200v-80h240v-240h80v240h240v80H520v240h-80v-240Z" />
			</svg>
			<span class="ml-2">Select</span>
		</div>
	</div>
</template>
