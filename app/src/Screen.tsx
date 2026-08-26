import { useCallback, useMemo, useState } from "react";
import { NetworkCard } from "./NetworkCard";
import { Visibility } from "./Visibility";
import { DeviceDetail } from "./topology/DeviceDetail";
import { DiscoveryProgress, QualityBanner } from "./topology/DiscoveryProgress";
import { TopologyView } from "./topology/TopologyView";
import { searchDevices } from "./topology/search";
import { categoryLabel } from "./topology/visual";
import type {
  CapabilityMatrix,
  Category,
  Device,
  DiscoveryReport,
  GroupView,
  NetworkIdentityReport,
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
  /** Resolves a group page. Live: a host command. Preview: a lookup. */
  getGroup: (category: Category, page: number) => Promise<GroupView>;
}

/** Shape of a dumped fixture payload. */
export interface PreviewData {
  fixture: string;
  identity: NetworkIdentityReport;
  capabilities: CapabilityMatrix;
  report: DiscoveryReport;
  group_pages: Record<string, GroupView[]>;
}

export function Screen({ data, live }: { data?: PreviewData; live?: ScreenData }) {
  const state: ScreenData = live ?? fromPreview(data!);
  const { identity, capabilities, overview, devices, report } = state;

  const [openCategory, setOpenCategory] = useState<Category | null>(null);
  const [group, setGroup] = useState<GroupView | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [query, setQuery] = useState("");

  const openGroup = useCallback(
    async (category: Category, page = 0) => {
      setOpenCategory(category);
      setGroup(await state.getGroup(category, page));
    },
    [state],
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
      <Brand />

      {/* 1. NETWORK, and 2. THIS DEVICE */}
      {identity ? (
        <NetworkCard report={identity} selfAddress={overview?.self_node?.device_id ?? null} />
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
              }}
              onSelectDevice={onSelect}
              onPage={(page) => openCategory && void openGroup(openCategory, page)}
            />
          ) : (
            <div className="topo-placeholder" />
          )}

          <div className="topo-side">
            {selectedDevice ? (
              <DeviceDetail device={selectedDevice} onClose={() => setSelected(null)} />
            ) : (
              <>
                {report && <QualityBanner quality={report.quality} />}
                <DiscoveryProgress sources={state.sources} done={report !== null} />
              </>
            )}
          </div>
        </div>
      </section>

      {/* 4. WHAT JRX CAN SEE */}
      {capabilities && <Visibility matrix={capabilities} />}
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
    getGroup: async (category, page) => {
      const pages = data.group_pages[category] ?? [];
      return pages[Math.min(page, pages.length - 1)] ?? pages[0]!;
    },
  };
}
