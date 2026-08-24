// Mirrors the serialised shapes from jrx-core. Host -> UI only; the renderer
// never sends these back (ARCHITECTURE.md §10).

export type CapabilityState =
  | { state: "observed"; mechanism: string }
  | { state: "available"; missing: string }
  | { state: "not_possible"; reason: string };

export interface CapabilityRow {
  probe: string;
  describes: string;
  state: CapabilityState;
}

export interface CapabilityMatrix {
  rows: CapabilityRow[];
  refused: { class: string }[];
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
