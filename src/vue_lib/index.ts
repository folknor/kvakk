export { type DisplayedItem, stateToDisplay, type ToDelete } from "./types";
export { type UtilsType, utils } from "./utils";

export function opt<T>(v?: T): T | null {
    return v ?? null;
}
