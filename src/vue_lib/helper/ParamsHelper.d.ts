import { UnlistenFn } from '@tauri-apps/api/event';

import { ToDelete } from '../types';

import { EndpointInfo } from '@/bindings/EndpointInfo';
import { OutboundPayload } from '@/bindings/OutboundPayload';
import { ChannelMessage } from '@/bindings/ChannelMessage';

export interface TauriVM {
    isAppInForeground: boolean;
    discoveryRunning: boolean;
    isDragHovering: boolean;
    requests: ChannelMessage[];
    endpointsInfo: EndpointInfo[];
    toDelete: ToDelete[];
    outboundPayload: OutboundPayload | undefined;
    unlisten: Array<UnlistenFn>;
    version: string | null;
    hostname: string | undefined;
    new_version: string | null;
    invoke: (cmd: string, args?: InvokeArgs) => Promise<unknown>

    displayedIsEmpty: boolean;
    displayedItems: DisplayedItem[];

    // Remapped function for compatibility with Tauri v1 and v2
    dialogOpen: (options?: {
        title: string,
        directory: boolean,
        multiple: boolean,
    }) => Promise<unknown>;
}
