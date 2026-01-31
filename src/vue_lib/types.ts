import type { ChannelMessage } from "@/bindings/ChannelMessage";
import type { DeviceType } from "@/bindings/DeviceType";
import type { TransferMetadata } from "@/bindings/TransferMetadata";
import type { TransferState } from "@/bindings/TransferState";

export interface ToDelete {
    id: string;
    triggered: number;
}

// Normalized request data extracted from ChannelMessage for easier frontend access
export interface NormalizedRequest {
    id: string;
    state: TransferState | null;
    metadata: TransferMetadata | null;
}

// Helper to extract client data from a ChannelMessage
export function normalizeChannelMessage(cm: ChannelMessage): NormalizedRequest {
    if ("Client" in cm.msg) {
        const client = cm.msg.Client;
        return {
            id: cm.id,
            state: client.state,
            metadata: client.metadata,
        };
    }
    // Lib messages don't have state/metadata
    return {
        id: cm.id,
        state: null,
        metadata: null,
    };
}

// Merge a new normalized request with an existing one, preserving non-null values
export function mergeNormalizedRequest(
    prev: NormalizedRequest,
    next: NormalizedRequest,
): NormalizedRequest {
    return {
        id: next.id,
        state: next.state ?? prev.state,
        metadata: next.metadata ?? prev.metadata,
    };
}

export interface DisplayedItem {
    id: string;
    name: string;
    deviceType: DeviceType;
    endpoint: boolean;

    state?: TransferState;
    pin_code?: string;
    files?: string[];
    text_description?: string;
    text_payload?: string;
    text_type?: string;
    destination?: string;
    total_bytes?: number;
    ack_bytes?: number;
}

export const stateToDisplay: Array<TransferState> = [
    "ReceivedPairedKeyResult",
    "WaitingForUserConsent",
    "ReceivingFiles",
    "Disconnected",
    "Finished",
    "SentIntroduction",
    "SendingFiles",
    "Cancelled",
    "Rejected",
];
