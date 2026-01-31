import type { ChannelMessage } from "@/bindings/ChannelMessage";
import type { SendInfo } from "@/bindings/SendInfo";
import type { TransferAction } from "@/bindings/TransferAction";
import type { TransferPayload } from "@/bindings/TransferPayload";
import type { TauriVM } from "./helper/ParamsHelper";
import {
    type DisplayedItem,
    type NormalizedRequest,
    stateToDisplay,
} from "./types";

// Helper to extract files from TransferPayload
function extractFiles(payload: TransferPayload | null): string[] | undefined {
    if (payload && "Files" in payload) {
        return payload.Files;
    }
    return;
}

// Helper to extract text description from payload
function extractTextDescription(
    payload: TransferPayload | null,
): string | undefined {
    if (!payload) return;
    if ("Text" in payload) return "Text content";
    if ("Url" in payload) return "URL";
    if ("Wifi" in payload) return `WiFi: ${payload.Wifi.ssid}`;
    return;
}

// Helper to extract text payload content
function extractTextPayload(
    payload: TransferPayload | null,
): string | undefined {
    if (!payload) return;
    if ("Text" in payload) return payload.Text;
    if ("Url" in payload) return payload.Url;
    if ("Wifi" in payload) return `SSID: ${payload.Wifi.ssid}`;
    return;
}

// Helper to extract text type
function extractTextType(payload: TransferPayload | null): string | undefined {
    if (!payload) return;
    if ("Text" in payload) return "Text";
    if ("Url" in payload) return "Url";
    if ("Wifi" in payload) return "Wifi";
    return;
}

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

    for (const el of vm.requests.filter((req: NormalizedRequest) =>
        stateToDisplay.includes(req.state ?? "Initial"),
    )) {
        const idx = ndisplayed.findIndex((nel) => el.id === nel.id);
        const meta = el.metadata;
        const payload = meta?.payload ?? null;

        const elem: DisplayedItem = {
            id: el.id,
            name: meta?.source?.name ?? "Unknown",
            deviceType: meta?.source?.device_type ?? "Unknown",
            endpoint: false,

            state: el.state ?? undefined,
            pin_code: meta?.pin_code ?? undefined,
            files: extractFiles(payload),
            text_description:
                meta?.payload_preview ?? extractTextDescription(payload),
            text_payload: extractTextPayload(payload),
            text_type: extractTextType(payload),
            ack_bytes: meta ? Number(meta.ack_bytes) : undefined,
            total_bytes: meta ? Number(meta.total_bytes) : undefined,
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

// Map frontend action names to Rust TransferAction
type FrontendAction = "AcceptTransfer" | "RejectTransfer" | "CancelTransfer";

function toTransferAction(action: FrontendAction): TransferAction {
    // biome-ignore lint/nursery/noUnnecessaryConditions: switch dispatches on value, not truthiness
    switch (action) {
        case "AcceptTransfer":
            return "ConsentAccept";
        case "RejectTransfer":
            return "ConsentDecline";
        case "CancelTransfer":
            return "TransferCancel";
    }
}

async function sendCmd(
    vm: TauriVM,
    id: string,
    action: FrontendAction,
): Promise<void> {
    const cm: ChannelMessage = {
        id: id,
        msg: { Lib: { action: toTransferAction(action) } },
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
