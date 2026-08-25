// Search over facts only.
//
// Matches what a device *is observed to be* — its address, hardware address,
// announced name and manufacturer — never anything JRX inferred about it and
// never a person. There is no owner field to search because JRX does not have
// one and will not guess at one.

import type { Device } from "../types";

/** The fields a query is matched against. Facts, in every case. */
function haystack(device: Device): string[] {
  const facts = device.facts;
  return [
    ...facts.addresses,
    facts.mac ?? "",
    facts.hostname ?? "",
    facts.vendor ?? "",
    ...facts.services,
    device.inference.category,
  ].filter(Boolean);
}

/** Filter devices by a free-text query.
 *
 *  An empty query returns everything rather than nothing: a search box that
 *  blanks the map when cleared is a trap. */
export function searchDevices(devices: Device[], query: string): Device[] {
  const needle = query.trim().toLowerCase();
  if (!needle) return devices;

  return devices.filter((device) =>
    haystack(device).some((field) => field.toLowerCase().includes(needle)),
  );
}

/** Whether a query would match a device. Exposed for highlighting. */
export function matches(device: Device, query: string): boolean {
  const needle = query.trim().toLowerCase();
  if (!needle) return false;
  return haystack(device).some((field) => field.toLowerCase().includes(needle));
}
