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

/** The physical link. A tunnel is a route, not a kind of link, so it is
 *  reported separately and never replaces this. */
export type ConnectionType = "wifi" | "ethernet" | "usb_tether" | "unknown";

/** A tunnel carrying the default route, over the physical connection. */
export interface Tunnel {
  interface: string;
  gateway: string | null;
  local_ip: string | null;
}

export interface ActiveInterface {
  interface: string;
  label: string | null;
  connection: ConnectionType;
  local_ip: string | null;
}

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
  /** Set when traffic leaves through a tunnel. The fields above continue to
   *  describe the physical network. */
  tunnel: Tunnel | null;
  other_active: ActiveInterface[];
}

export interface NetworkIdentityReport {
  identity: NetworkIdentity;
  observed_in_ms: number;
}

// ---- devices ----

export type Category =
  | "computers"
  | "phones"
  | "smart_home"
  | "infrastructure"
  | "unknown";

export type Confidence = "high" | "medium" | "none";

export type DiscoverySource =
  | "arp_cache"
  | "mdns"
  | "ssdp"
  | "self_interface"
  | "default_route";

export type EvidenceKind =
  | "mac_address"
  | "hostname"
  | "service_type"
  | "upnp_device_type"
  | "vendor"
  | "gateway_role"
  | "self_role";

export interface Evidence {
  kind: EvidenceKind;
  value: string;
  method: DiscoverySource;
}

/** Only what was observed. No conclusions. */
export interface ObservedFacts {
  addresses: string[];
  mac: string | null;
  hostname: string | null;
  vendor: string | null;
  services: string[];
  upnp_types: string[];
  sources: DiscoverySource[];
  mac_randomised: boolean;
}

export interface CategoryChange {
  from: Category;
  to: Category;
  confidence: Confidence;
  reason: string;
  triggered_by: Evidence;
}

/** What JRX decided, and why. Kept apart from the facts on purpose. */
export interface CategoryInference {
  category: Category;
  confidence: Confidence;
  family: string | null;
  rationale: string;
  /** Only the evidence that produced the conclusion. Never a vendor. */
  supporting: Evidence[];
  history: CategoryChange[];
}

export interface Device {
  id: string;
  facts: ObservedFacts;
  inference: CategoryInference;
  evidence: Evidence[];
  is_self: boolean;
  is_gateway: boolean;
}

// ---- topology ----

/** Carries everything needed to draw a node and explain it in place. */
export interface TopologyNode {
  device_id: string;
  display_name: string;
  category: Category;
  confidence: Confidence;
  family: string | null;
  rationale: string;
  evidence: Evidence[];
  vendor: string | null;
  mac_randomised: boolean;
  sources: DiscoverySource[];
  is_self: boolean;
  is_gateway: boolean;
}

/** A measured observation about a group — never a category. */
export interface GroupFact {
  count: number;
  description: string;
  is_category: boolean;
}

export interface CategorySummary {
  category: Category;
  label: string;
  count: number;
  facts: GroupFact[];
  facts_are_independent: boolean;
  collapsed_by_default: boolean;
}

export interface TopologyOverview {
  center: TopologyNode | null;
  self_node: TopologyNode | null;
  groups: CategorySummary[];
  total: number;
}

export interface GroupView {
  category: Category;
  label: string;
  total: number;
  facts: GroupFact[];
  facts_are_independent: boolean;
  page: number;
  page_size: number;
  page_count: number;
  devices: TopologyNode[];
}

// ---- discovery ----

export type SourceStatus =
  | { status: "ok"; observations: number }
  | { status: "failed"; reason: string };

export interface SourceQuality {
  method: DiscoverySource;
  label: string;
  status: SourceStatus;
  observations: number;
  names_resolved: number;
  services_seen: number;
}

export type DiscoveryVerdict =
  | "healthy"
  | "degraded"
  | "discovery_blocked"
  | "network_appears_empty";

export type LocalNetworkInference = "working" | "likely_blocked" | "undetermined";

export interface DiscoveryQuality {
  verdict: DiscoveryVerdict;
  explanation: string;
  sources: SourceQuality[];
  local_network: LocalNetworkInference;
}

export type Isolation = "likely_isolated" | "normal";

export interface DiscoverySummary {
  total: number;
  unidentified: number;
  by_category: [Category, number][];
  isolation: Isolation;
}

export interface DiscoveryReport {
  devices: Device[];
  overview: TopologyOverview;
  summary: DiscoverySummary;
  quality: DiscoveryQuality;
  took_ms: number;
}

/** Streamed while discovery runs, so the map is never a blank screen. */
export type DiscoveryStage =
  | { stage: "started" }
  | { stage: "source_finished"; source: SourceQuality }
  | { stage: "partial"; overview: TopologyOverview; devices: Device[] };
