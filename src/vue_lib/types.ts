import { State } from '@/bindings/State';
import { DeviceType } from '@/bindings/DeviceType';

export interface ToDelete {
	id: string,
	triggered: number
}

export interface DisplayedItem {
	id: string,
	name: string,
	deviceType: DeviceType,
	endpoint: boolean,

	state?: State,
	pin_code?: string,
	files?: string[],
	text_description?: string,
	text_payload?: string,
	text_type?: string,
	destination?: string,
	total_bytes?: number,
	ack_bytes?: number,
}

export const stateToDisplay: Array<Partial<State>> = ["ReceivedPairedKeyResult", "WaitingForUserConsent", "ReceivingFiles", "Disconnected",
	"Finished", "SentIntroduction", "SendingFiles", "Cancelled", "Rejected"]

export interface Toast {
	id: number;
	type: ToastType;
	message: string;
}

export enum ToastType {
	Success = "SUCCESS",
	Error = "ERROR",
	Info = "INFO",
}
