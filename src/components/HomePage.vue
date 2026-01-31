<template>
	<div class="flex flex-col w-full h-full bg-green-50 overflow-hidden">
		<Heading :vm="vm" />

		<div class="flex-1 flex flex-col bg-white w-full rounded-t-3xl p-8 overflow-y-auto">
				<ContentStatus :vm="vm" @outbound-payload="(el: OutboundPayload) => outboundPayload = el" @discovery-running="discoveryRunning = true;" />

				<div
					v-for="item in displayedItems" :key="item.id" class="w-full rounded-3xl flex flex-row gap-6 p-4 mb-4 bg-green-100"
					:class="{'cursor-pointer': item.endpoint}" @click="item.endpoint && sendInfo(vm, item.id)">
					<!-- Loader and image of the device type & pin_code -->
					<ItemSide :item="item" />

					<!-- Content and state of the transfer -->
					<div class="flex-1 flex flex-col text-sm min-w-0" :class="{'justify-center': item.state === undefined}">
						<h4 class="text-base font-medium">
							{{ item.name }}
						</h4>

						<div v-if="item.state === 'WaitingForUserConsent'" class="flex-1 flex flex-col justify-between">
							<p class="mt-4">
								Wants to share {{ item.files?.join(', ') ?? item.text_description ?? 'some file(s).' }}
							</p>
							<div class="flex flex-row justify-end gap-4 mt-1">
								<p
									@click.stop="sendCmd(vm, item.id, 'AcceptTransfer')" class="btn px-3
									rounded-xl active:scale-95 transition duration-150 ease-in-out shadow-none">
									Accept
								</p>
								<p
									@click.stop="sendCmd(vm, item.id, 'RejectTransfer')" class="btn px-3
									rounded-xl active:scale-95 transition duration-150 ease-in-out shadow-none">
									Decline
								</p>
							</div>
						</div>

						<div v-else-if="['SentIntroduction', 'SendingFiles', 'ReceivingFiles'].includes(item.state ?? 'Initial')">
							<p class="mt-2" v-if="['SentIntroduction', 'SendingFiles'].includes(item.state ?? 'Initial')">
								Sending...
							</p>
							<p class="mt-2" v-else>
								Receiving...
							</p>
							<p v-for="f in item.files ?? []" :key="f" class="overflow-hidden whitespace-nowrap text-ellipsis">
								{{ f }}
							</p>
							<div class="flex flex-row justify-end gap-4 mt-1">
								<p
									@click.stop="sendCmd(vm, item.id, 'CancelTransfer')" class="btn px-3
									rounded-xl active:scale-95 transition duration-150 ease-in-out shadow-none">
									Cancel
								</p>
							</div>
						</div>

						<div v-else-if="item.state === 'Finished'">
							<p class="mt-2">
								Received <span v-if="item.text_type">text</span>
							</p>

							<!-- If files -->
							<p v-for="f in item.files ?? []" :key="f" class="overflow-hidden whitespace-nowrap text-ellipsis">
								{{ f }}
							</p>
							<p v-if="item.files" class="mt-2 overflow-hidden whitespace-nowrap text-ellipsis">
								<span v-if="item.files">Saved to </span>{{ item.destination }}
							</p>

							<!-- If text -->
							<p v-if="item.text_type" class="!select-text cursor-text overflow-hidden whitespace-nowrap text-ellipsis">
								{{ item.text_payload }}
							</p>

							<div class="flex flex-row justify-end gap-4 mt-1">
								<p
									@click.stop="removeRequest(vm, item.id)"
									class="btn px-3 rounded-xl active:scale-95 transition duration-150 ease-in-out shadow-none">
									Clear
								</p>
							</div>
						</div>

						<div v-else-if="item.state === 'Cancelled'">
							<p class="mt-2">
								Transfer cancelled
							</p>
							<div class="flex flex-row justify-end gap-4 mt-1">
								<p
									@click.stop="removeRequest(vm, item.id)" class="btn px-3
									rounded-xl active:scale-95 transition duration-150 ease-in-out shadow-none">
									Clear
								</p>
							</div>
						</div>

						<div v-else-if="item.state === 'Rejected'">
							<p class="mt-2">
								Transfer rejected
							</p>
							<div class="flex flex-row justify-end gap-4 mt-1">
								<p
									@click.stop="removeRequest(vm, item.id)" class="btn px-3
									rounded-xl active:scale-95 transition duration-150 ease-in-out shadow-none">
									Clear
								</p>
							</div>
						</div>

						<div v-else-if="item.state === 'Disconnected'">
							<p class="mt-2">
								Unexpected disconnection
							</p>
							<div class="flex flex-row justify-end gap-4 mt-1">
								<p
									@click.stop="removeRequest(vm, item.id)" class="btn px-3
									rounded-xl active:scale-95 transition duration-150 ease-in-out shadow-none">
									Clear
								</p>
							</div>
						</div>
					</div>
				</div>
			</div>
		</div>
</template>

<script lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open as tauriDialog } from "@tauri-apps/plugin-dialog";
import { nextTick, ref } from "vue";

import type { ChannelMessage } from "@/bindings/ChannelMessage";
import type { EndpointInfo } from "@/bindings/EndpointInfo";
import type { OutboundPayload } from "@/bindings/OutboundPayload";
import ContentStatus from "../composables/ContentStatus.vue";

import Heading from "../composables/Heading.vue";
import ItemSide from "../composables/ItemSide.vue";
import {
    type DisplayedItem,
    mergeNormalizedRequest,
    type NormalizedRequest,
    normalizeChannelMessage,
    opt,
    stateToDisplay,
    type TauriVM,
    type ToDelete,
    utils,
} from "../vue_lib";

export default {
    name: "HomePage",

    components: {
        Heading,
        ContentStatus,
        ItemSide,
    },

    setup(): {
        stateToDisplay: typeof stateToDisplay;
        invoke: typeof invoke;
        dialogOpen: typeof tauriDialog;
        _displayedItems: typeof utils._displayedItems;
        clearSending: typeof utils.clearSending;
        removeRequest: typeof utils.removeRequest;
        sendInfo: typeof utils.sendInfo;
        sendCmd: typeof utils.sendCmd;
        blured: typeof utils.blured;
        getProgress: typeof utils.getProgress;
    } {
        const dialogOpen = tauriDialog;

        return {
            stateToDisplay,
            invoke,
            dialogOpen,
            ...utils,
        };
    },

    data(): {
        isAppInForeground: boolean;
        discoveryRunning: boolean;
        isDragHovering: boolean;
        requests: NormalizedRequest[];
        endpointsInfo: EndpointInfo[];
        toDelete: ToDelete[];
        outboundPayload: OutboundPayload | undefined;
        cleanupInterval: NodeJS.Timeout | null;
        unlisten: UnlistenFn[];
        hostname: string | undefined;
        backendReady: boolean;
    } {
        return {
            isAppInForeground: false,
            discoveryRunning: ref(false) as unknown as boolean,
            isDragHovering: ref(false) as unknown as boolean,

            requests: ref<NormalizedRequest[]>(
                [],
            ) as unknown as NormalizedRequest[],
            endpointsInfo: ref<EndpointInfo[]>([]) as unknown as EndpointInfo[],
            toDelete: ref<ToDelete[]>([]) as unknown as ToDelete[],
            outboundPayload: ref<OutboundPayload | undefined>() as unknown as
                | OutboundPayload
                | undefined,

            cleanupInterval: opt<NodeJS.Timeout>(),
            unlisten: [] as UnlistenFn[],

            hostname: ref<string>() as unknown as string | undefined,
            backendReady: ref(false) as unknown as boolean,
        };
    },

    mounted(): void {
        void nextTick(async () => {
            this.hostname = (await invoke("get_hostname")) as string;

            this.unlisten.push(
                await listen("rs2js_channelmessage", (event) => {
                    const cm = event.payload as ChannelMessage;
                    const normalized = normalizeChannelMessage(cm);
                    const idx = this.requests.findIndex(
                        (el) => el.id === cm.id,
                    );

                    if (normalized.state === "Disconnected") {
                        this.toDelete.push({
                            id: cm.id,
                            triggered: Date.now(),
                        });
                    }

                    if (idx !== -1) {
                        const prev = this.requests[idx];
                        if (prev) {
                            this.requests.splice(
                                idx,
                                1,
                                mergeNormalizedRequest(prev, normalized),
                            );
                        }
                    } else {
                        this.requests.push(normalized);
                    }
                }),
            );

            this.unlisten.push(
                await listen("rs2js_endpointinfo", (event) => {
                    const ei = event.payload as EndpointInfo;
                    const idx = this.endpointsInfo.findIndex(
                        (el) => el.id === ei.id,
                    );

                    if (!ei.present) {
                        if (idx !== -1) {
                            this.endpointsInfo.splice(idx, 1);
                        }

                        return;
                    }

                    if (idx !== -1) {
                        this.endpointsInfo.splice(idx, 1, ei);
                    } else {
                        this.endpointsInfo.push(ei);
                    }
                }),
            );

            this.unlisten.push(
                await getCurrentWindow().onDragDropEvent(async (event) => {
                    if (event.payload.type === "over") {
                        this.isDragHovering = true;
                    } else if (event.payload.type === "drop") {
                        this.isDragHovering = false;
                        this.outboundPayload = {
                            Files: event.payload.paths,
                        } as OutboundPayload;
                        if (!this.discoveryRunning) {
                            await invoke("start_discovery");
                        }
                        this.discoveryRunning = true;
                    } else {
                        this.isDragHovering = false;
                    }
                }),
            );

            this.backendReady = (await invoke("is_ready")) as boolean;

            this.unlisten.push(
                await listen("backend_ready", () => {
                    this.backendReady = true;
                }),
            );
        });
    },

    unmounted(): void {
        for (const fn of this.unlisten) {
            fn();
        }

        if (this.cleanupInterval?.[Symbol.dispose]) {
            this.cleanupInterval[Symbol.dispose]();
        }
    },

    computed: {
        vm(): TauriVM {
            return this as unknown as TauriVM;
        },
        displayedIsEmpty(): boolean {
            return this.displayedItems.length === 0;
        },
        displayedItems(): Array<DisplayedItem> {
            return this._displayedItems(this);
        },
    },

    methods: {},
};
</script>
