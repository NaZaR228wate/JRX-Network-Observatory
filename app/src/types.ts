// Mirrors the serialised shapes from jrx-core. Host -> UI only; the renderer
// never sends these back (ARCHITECTURE.md §10).

export type Permission = "local_network" | "location_services";

export type PermissionState =
  | "granted"
  | "denied"
  | "not_requested"
  | "unknown";

/** Whether the OS actually told us, or we simply cannot ask. */
export type Certainty = "confirmed" | "unverifiable";

export type CapabilityState =
  | { state: "observed"; mechanism: string }
  | { state: "available"; missing: Permission; certainty: Certainty }
  | { state: "not_possible"; reason: string };

export interface CapabilityRow {
  probe: string;
  describes: string;
  state: CapabilityState;
}

export interface PermissionInfo {
  permission: Permission;
  label: string;
  grant_hint: string;
  queryable: boolean;
  state: PermissionState;
}

export interface CapabilityMatrix {
  rows: CapabilityRow[];
  refused: { class: string; rationale: string }[];
  limitations: { describes: string; reason: string }[];
  permissions: PermissionInfo[];
}

export type ConnectionType =
  | "wifi"
  | "ethernet"
  | "usb_tether"
  | "vpn"
  | "unknown";

export type Band = "ghz2_4" | "ghz5" | "ghz6";

export interface WifiDetails {
  ssid: string | null;
  bssid: string | null;
  channel: number | null;
  band: Band | null;
  signal_dbm: number | null;
  noise_dbm: number | null;
  security: string | null;
  phy_mode: string | null;
}

// Every case is a distinct state with its own explanation — never one empty
// result (ARCHITECTURE.md §12).
export type WifiStatus =
  | { status: "no_hardware" }
  | { status: "radio_off" }
  | { status: "not_associated" }
  | ({ status: "associated" } & WifiDetails)
  | { status: "unavailable"; reason: string }
  | { status: "permission_required" };

export interface NetworkIdentity {
  connection: ConnectionType;
  interface: string;
  interface_label: string | null;
  local_ip: string | null;
  subnet: { network: string; prefix_len: number } | null;
  gateway: string | null;
  dns_servers: string[];
  wifi: WifiStatus;
  vpn_active: boolean;
}

export interface NetworkIdentityReport {
  identity: NetworkIdentity;
  observed_in_ms: number;
}
