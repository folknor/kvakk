export type { TauriVM } from "./helper/ParamsHelper";
export {
    type DisplayedItem,
    mergeNormalizedRequest,
    type NormalizedRequest,
    normalizeChannelMessage,
    stateToDisplay,
    type ToDelete,
} from "./types";
export { type UtilsType, utils } from "./utils";

export function opt<T>(v?: T): T | null {
    return v ?? null;
}
