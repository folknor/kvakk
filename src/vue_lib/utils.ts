import type { ChannelAction } from "@/bindings/ChannelAction";
import type { ChannelMessage } from "@/bindings/ChannelMessage";
import type { SendInfo } from "@/bindings/SendInfo";
import type { TauriVM } from "./helper/ParamsHelper";
import { type DisplayedItem, stateToDisplay } from "./types";

function _displayedItems(vm: TauriVM): Array<DisplayedItem> {
    const ndisplayed: DisplayedItem[] = [];

    for (const el of vm.endpointsInfo) {
        const idx = ndisplayed.findIndex((nel) => el.id === nel.id);
        if (idx !== -1) continue;

        ndisplayed.push({
            id: el.id,
            name: el.name ?? "Unknown",
            deviceType: el.rtype ?? "Unknown",
            endpoint: true,
        });
    }

    for (const el of vm.requests.filter((req) =>
        stateToDisplay.includes(req.state ?? "Initial"),
    )) {
        const idx = ndisplayed.findIndex((nel) => el.id === nel.id);
        const elem: DisplayedItem = {
            id: el.id,
            name: el.meta?.source?.name ?? "Unknown",
            deviceType: el.meta?.source?.device_type ?? "Unknown",
            endpoint: false,

            state: el.state ?? undefined,
            pin_code: el.meta?.pin_code ?? undefined,
            destination: el.meta?.destination ?? undefined,
            files: el.meta?.files ?? undefined,
            text_description: el.meta?.text_description ?? undefined,
            text_payload: el.meta?.text_payload ?? undefined,
            text_type: el.meta?.text_type ?? undefined,
            ack_bytes: (el.meta?.ack_bytes as number | undefined) ?? undefined,
            total_bytes:
                (el.meta?.total_bytes as number | undefined) ?? undefined,
        };

        if (idx !== -1) {
            ndisplayed.splice(idx, 1, elem);
        } else {
            ndisplayed.push(elem);
        }
    }

    return ndisplayed;
}

async function clearSending(vm: TauriVM): Promise<void> {
    await vm.invoke("stop_discovery");
    vm.outboundPayload = undefined;
    vm.discoveryRunning = false;
    vm.endpointsInfo = [];
}

function removeRequest(vm: TauriVM, id: string): void {
    const idx = vm.requests.findIndex((el) => el.id === id);

    if (idx !== -1) {
        vm.requests.splice(idx, 1);
    }
}

async function sendInfo(vm: TauriVM, eid: string): Promise<void> {
    if (vm.outboundPayload === undefined) return;

    const ei = vm.endpointsInfo.find((el) => el.id === eid);
    if (!(ei?.ip && ei.port)) return;

    const msg: SendInfo = {
        id: ei.id,
        name: ei.name ?? "Unknown",
        addr: `${ei.ip}:${ei.port}`,
        ob: vm.outboundPayload,
    };

    await vm.invoke("send_payload", { message: msg });
}

async function sendCmd(
    vm: TauriVM,
    id: string,
    action: ChannelAction,
): Promise<void> {
    const cm: ChannelMessage = {
        id: id,
        direction: "FrontToLib",
        action: action,
        meta: null,
        state: null,
        rtype: null,
    };

    await vm.invoke("send_to_rs", { message: cm });
}

function blured(): void {
    (document.activeElement as HTMLElement | null)?.blur();
}

function getProgress(item: DisplayedItem): string {
    const ack = item.ack_bytes ?? 0;
    const total = item.total_bytes ?? 1;
    const value = (ack / total) * 100;
    return `--progress: ${value}`;
}

export const utils: {
    _displayedItems: typeof _displayedItems;
    clearSending: typeof clearSending;
    removeRequest: typeof removeRequest;
    sendInfo: typeof sendInfo;
    sendCmd: typeof sendCmd;
    blured: typeof blured;
    getProgress: typeof getProgress;
} = {
    _displayedItems,
    clearSending,
    removeRequest,
    sendInfo,
    sendCmd,
    blured,
    getProgress,
};
export type UtilsType = typeof utils;
