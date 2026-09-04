import { useCallback, useMemo, useState } from "react";
import { Activity } from "./activity/Activity";
import { NetworkCard } from "./NetworkCard";
import { Visibility } from "./Visibility";
import { DeviceDetail } from "./topology/DeviceDetail";
import { DiscoveryProgress, QualityBanner } from "./topology/DiscoveryProgress";
import { Onboarding } from "./Onboarding";
import { TopologyView } from "./topology/TopologyView";
import { searchDevices } from "./topology/search";
import { categoryLabel } from "./topology/visual";
import type {
  ActivitySnapshot,
  CapabilityMatrix,
  Category,
  Device,
  DiscoveryReport,
  GroupView,
  NetworkIdentityReport,
  RecognitionUpdate,
  SourceQuality,
  TopologyNode,
  TopologyOverview,
} from "./types";

/** Everything the screen needs. Supplied live by the host, or by a fixture in
 *  the development preview — the same components either way. */
export interface ScreenData {
  identity: NetworkIdentityReport | null;
  capabilities: CapabilityMatrix | null;
  overview: TopologyOverview | null;
  devices: Device[];
  report: DiscoveryReport | null;
  sources: SourceQuality[];
  failure: string | null;
  activity: ActivitySnapshot | null;
  recognition: RecognitionUpdate | null;
  /** Resolves a group page. Live: a host command. Preview: a lookup. */
  getGroup: (category: Category, page: number, filterKey: string) => Promise<GroupView>;
  /** Erase everything JRX has remembered. Live: a host command. */
  forget: () => Promise<void>;
}

/** Shape of a dumped fixture payload. */
export interface PreviewData {
  fixture: string;
  identity: NetworkIdentityReport;
  capabilities: CapabilityMatrix;
  report: DiscoveryReport;
  group_pages: Record<string, Record<string, GroupView[]>>;
  /** Present only in the development preview. */
  activity?: ActivitySnapshot;
}

export function Screen({ data, live }: { data?: PreviewData; live?: ScreenData }) {
  const state: ScreenData = live ?? fromPreview(data!);
  const { identity, capabilities, overview, devices, report, recognition } = state;

  const [openCategory, setOpenCategory] = useState<Category | null>(null);
  const [group, setGroup] = useState<GroupView | null>(null);
  const [filterKey, setFilterKey] = useState("all");
  const [selected, setSelected] = useState<string | null>(null);
  const [query, setQuery] = useState("");

  const openGroup = useCallback(
    async (category: Category, page = 0, key = "all") => {
      setOpenCategory(category);
      setFilterKey(key);
      setGroup(await state.getGroup(category, page, key));
    },
    [state],
  );

  // Only surface "new" once the network itself is recognised: on a first
  // visit every device is trivially new, which would be noise, not signal.
  const newDevices = useMemo(
    () =>
      new Set(
        recognition?.network && recognition.network !== "first_time"
          ? recognition.new_device_ids
          : [],
      ),
    [recognition],
  );

  const matched = useMemo(() => searchDevices(devices, query), [devices, query]);
  const highlighted = useMemo(() => new Set(matched.map((d) => d.id)), [matched]);
  const selectedDevice = useMemo(
    () => devices.find((d) => d.id === selected) ?? null,
    [devices, selected],
  );
  const onSelect = useCallback((node: TopologyNode) => setSelected(node.device_id), []);

  if (state.failure) {
    return (
      <div className="shell">
        <Brand />
        <div className="banner warn">
          <strong>JRX could not read this network.</strong>
          <div className="note">{state.failure}</div>
        </div>
      </div>
    );
  }

  return (
    <div className="shell">
      <Onboarding />
      <Brand />

      {/* 1. NETWORK, and 2. THIS DEVICE */}
      {identity ? (
        <NetworkCard
          report={identity}
          recognition={recognition}
          // From the identity, not from discovery: this address is known in
          // the first few hundred milliseconds, and showing "unknown" while
          // discovery runs would be wrong for four seconds.
          selfAddress={identity.identity.local_ip ?? overview?.self_node?.device_id ?? null}
        />
      ) : (
        <section className="netcard skeleton" aria-hidden="true" />
      )}

      {/* 3. DEVICES AROUND YOU */}
      <section className="devices">
        <div className="devices-head">
          <div>
            <h3>Devices around you</h3>
            <p className="note">
              {overview
                ? `${overview.total} observed · your router is in the centre`
                : "Reading what your computer already knows…"}
            </p>
            {overview &&
              recognition?.network &&
              recognition.network !== "first_time" &&
              recognition.new_here > 0 && (
                <p className="note new-here">
                  {recognition.new_here}{" "}
                  {recognition.new_here === 1 ? "device you have" : "devices you have"} not
                  seen on this network before
                </p>
              )}
          </div>
          <input
            className="search"
            type="search"
            placeholder="Search name, address, manufacturer…"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            aria-label="Search devices by observed facts"
          />
        </div>

        {overview && <CategoryCounts overview={overview} onOpen={(c) => void openGroup(c)} />}


        {query && (
          <p className="note search-result">
            {matched.length} of {devices.length} match. JRX searches only what it
            observed — it has no owner information to search.
          </p>
        )}

        <div className="topo-body">
          {overview ? (
            <TopologyView
              overview={overview}
              group={group}
              openCategory={openCategory}
              highlighted={highlighted}
              searching={query.trim().length > 0}
              onOpenGroup={(c) => void openGroup(c)}
              onCloseGroup={() => {
                setOpenCategory(null);
                setGroup(null);
                setFilterKey("all");
              }}
              onSelectDevice={onSelect}
              newDevices={newDevices}
              filterKey={filterKey}
              onFilter={(key) => openCategory && void openGroup(openCategory, 0, key)}
              onPage={(page) => openCategory && void openGroup(openCategory, page, filterKey)}
            />
          ) : (
            <div className="topo-placeholder" />
          )}

          <div className="topo-side">
            {selectedDevice ? (
              <DeviceDetail
                device={selectedDevice}
                isNew={newDevices.has(selectedDevice.id)}
                onClose={() => setSelected(null)}
              />
            ) : (
              <>
                {report && (
                  <QualityBanner
                    quality={report.quality}
                    isolation={report.summary.isolation}
                  />
                )}
                <DiscoveryProgress sources={state.sources} done={report !== null} />
              </>
            )}
          </div>
        </div>
      </section>

      {/* 4. THIS MAC'S OWN ACTIVITY */}
      <Activity snapshot={state.activity} />

      {/* 5. WHAT JRX CAN SEE */}
      {capabilities && <Visibility matrix={capabilities} />}

      {/* 6. WHAT JRX REMEMBERS, AND HOW TO ERASE IT */}
      <MemoryFooter forget={state.forget} />
    </div>
  );
}

/** Counts before the map, so the shape of the network reads without decoding
 *  a diagram first. */
function CategoryCounts({
  overview,
  onOpen,
}: {
  overview: TopologyOverview;
  onOpen: (category: Category) => void;
}) {
  const routerInCentre = overview.center !== null;

  return (
    <ul className="counts">
      {overview.groups.map((group) => {
        const centreOnly =
          group.category === "infrastructure" && group.count === 0 && routerInCentre;
        const empty = group.count === 0 && !centreOnly;

        return (
          <li key={group.category} className={empty ? "empty" : undefined}>
            <button
              disabled={group.count === 0}
              onClick={() => onOpen(group.category)}
              aria-label={`${categoryLabel(group.category)}, ${group.count} devices`}
            >
              <span className={`swatch cat-${group.category.replace("_", "-")}`} />
              <span className="count">{centreOnly ? 1 : group.count}</span>
              <span className="count-label">{categoryLabel(group.category)}</span>
              {/* The router is drawn in the centre, so Infrastructure would
                  otherwise read as empty when its only member is the router. */}
              {centreOnly && <span className="count-note">router, shown in centre</span>}
            </button>
          </li>
        );
      })}
    </ul>
  );
}

function Brand() {
  return (
    <div className="brand">
      <h1>JRX</h1>
      <span className="tag">Network Observatory</span>
    </div>
  );
}

/** What JRX keeps between runs, and a real way to erase it. The store holds
 *  one-way fingerprints of networks and devices, never their names or
 *  addresses, and never leaves this Mac (ADR-021). */
function MemoryFooter({ forget }: { forget: () => Promise<void> }) {
  const [stage, setStage] = useState<"idle" | "confirm" | "done">("idle");

  return (
    <footer className="memory-footer">
      <p className="note">
        JRX remembers only on this Mac — one-way fingerprints of the networks and
        devices it has seen, never their names or addresses, so it can tell you
        when something is new. Nothing is sent anywhere.
      </p>
      {stage === "done" ? (
        <p className="note">JRX has forgotten every network and device it had remembered.</p>
      ) : stage === "idle" ? (
        <button className="linklike" onClick={() => setStage("confirm")}>
          Forget what JRX has learned
        </button>
      ) : (
        <span className="forget-confirm">
          Erase all remembered networks and devices?{" "}
          <button
            className="linklike danger"
            onClick={async () => {
              await forget();
              setStage("done");
            }}
          >
            Forget everything
          </button>{" "}
          <button className="linklike" onClick={() => setStage("idle")}>
            Cancel
          </button>
        </span>
      )}
    </footer>
  );
}

/** Adapt a dumped fixture into the same shape the live host provides. */
function fromPreview(data: PreviewData): ScreenData {
  return {
    identity: data.identity,
    capabilities: data.capabilities,
    overview: data.report.overview,
    devices: data.report.devices,
    report: data.report,
    sources: data.report.quality.sources,
    failure: null,
    activity: data.activity ?? null,
    recognition: null,
    forget: async () => {},
    getGroup: async (category, page, filterKey) => {
      const pages = data.group_pages[category]?.[filterKey] ?? [];
      return pages[Math.min(page, Math.max(0, pages.length - 1))] ?? pages[0]!;
    },
  };
}
