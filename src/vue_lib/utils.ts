import { TauriVM } from './helper/ParamsHelper';
import { DisplayedItem, stateToDisplay } from './types';
import { SendInfo } from '@/bindings/SendInfo';
import { ChannelMessage } from '@/bindings/ChannelMessage';
import { ChannelAction } from '@martichou/core_lib';
import { gt } from 'semver';

function _displayedItems(vm: TauriVM): Array<DisplayedItem> {
	const ndisplayed = new Array<DisplayedItem>();

	vm.endpointsInfo.forEach((el) => {
		const idx = ndisplayed.findIndex((nel) => el.id == nel.id);
		if (idx !== -1) return;

		ndisplayed.push({
			id: el.id,
			name: el.name ?? 'Unknown',
			deviceType: el.rtype ?? 'Unknown',
			endpoint: true,
		})
	});

	vm.requests.filter((el) => stateToDisplay.includes(el.state ?? 'Initial')).forEach((el) => {
		const idx = ndisplayed.findIndex((nel) => el.id == nel.id);
		const elem: DisplayedItem = {
			id: el.id,
			name: el.meta?.source?.name ?? 'Unknown',
			deviceType: el.meta?.source?.device_type ?? 'Unknown',
			endpoint: false,

			state: el.state ?? undefined,
			pin_code: el.meta?.pin_code ?? undefined,
			destination: el.meta?.destination ?? undefined,
			files: el.meta?.files ?? undefined,
			text_description: el.meta?.text_description ?? undefined,
			text_payload: el.meta?.text_payload ?? undefined,
			text_type: el.meta?.text_type ?? undefined,
			ack_bytes: (el.meta?.ack_bytes as number | undefined) ?? undefined,
			total_bytes: (el.meta?.total_bytes as number | undefined) ?? undefined,
		};

		if (idx !== -1) {
			ndisplayed.splice(idx, 1, elem);
		} else {
			ndisplayed.push(elem)
		}
	});

	return ndisplayed;
}

async function clearSending(vm: TauriVM) {
	await vm.invoke('stop_discovery');
	vm.outboundPayload = undefined;
	vm.discoveryRunning = false;
	vm.endpointsInfo = [];
}

function removeRequest(vm: TauriVM, id: string) {
	const idx = vm.requests.findIndex((el) => el.id === id);

	if (idx !== -1) {
		vm.requests.splice(idx, 1);
	}
}

async function sendInfo(vm: TauriVM, eid: string) {
	if (vm.outboundPayload === undefined) return;

	const ei = vm.endpointsInfo.find((el) => el.id === eid);
	if (!ei || !ei.ip || !ei.port) return;

	const msg: SendInfo = {
		id: ei.id,
		name: ei.name ?? 'Unknown',
		addr: ei.ip + ":" + ei.port,
		ob: vm.outboundPayload,
	};

	await vm.invoke('send_payload', { message: msg });
}

async function sendCmd(vm: TauriVM, id: string, action: ChannelAction) {
	const cm: ChannelMessage = {
		id: id,
		direction: 'FrontToLib',
		action: action,
		meta: null,
		state: null,
		rtype: null,
	};
	console.log("js2rs:", cm);

	await vm.invoke('send_to_rs', { message: cm });
}

function blured() {
	(document.activeElement as any).blur();
}

function getProgress(item: DisplayedItem): string {
	const value = item.ack_bytes! / item.total_bytes! * 100;
	return `--progress: ${value}`;
}

async function getLatestVersion(vm: TauriVM) {
	try {
		const response = await fetch('https://api.github.com/repos/martichou/rquickshare/releases/latest');
		if (!response.ok) {
			throw new Error(`Error: ${response.status} ${response.statusText}`);
		}
		const data = await response.json();
		const new_version = data.tag_name.substring(1);
		console.log(`Latest version: ${vm.version} vs ${new_version}`);

		if (vm.version && gt(new_version, vm.version)) {
			vm.new_version = new_version;
		}
	} catch (err) {
		console.error(err);
	}
}

// Default export
export const utils = {
	_displayedItems,
	clearSending,
	removeRequest,
	sendInfo,
	sendCmd,
	blured,
	getProgress,
	getLatestVersion,
};
export type UtilsType = typeof utils;
