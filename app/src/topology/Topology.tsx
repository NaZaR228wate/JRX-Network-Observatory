import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  Category,
  Device,
  DiscoveryReport,
  DiscoveryStage,
  GroupView,
  SourceQuality,
  TopologyNode,
  TopologyOverview,
} from "../types";
import { DeviceDetail } from "./DeviceDetail";
import { DiscoveryProgress, QualityBanner } from "./DiscoveryProgress";
import { TopologyView } from "./TopologyView";
import { searchDevices } from "./search";

export function Topology() {
  const [overview, setOverview] = useState<TopologyOverview | null>(null);
  const [report, setReport] = useState<DiscoveryReport | null>(null);
  const [devices, setDevices] = useState<Device[]>([]);
  const [sources, setSources] = useState<SourceQuality[]>([]);
  const [failure, setFailure] = useState<string | null>(null);

  const [openCategory, setOpenCategory] = useState<Category | null>(null);
  const [group, setGroup] = useState<GroupView | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [query, setQuery] = useState("");

  useEffect(() => {
    const unlisteners = [
      listen<DiscoveryStage>("discovery://stage", (event) => {
        const stage = event.payload;
        if (stage.stage === "source_finished") {
          setSources((prev) =>
            prev.some((s) => s.method === stage.source.method)
              ? prev
              : [...prev, stage.source],
          );
        } else if (stage.stage === "partial") {
          // The neighbour cache returns in milliseconds; showing it at once is
          // what keeps the first seconds from being a blank screen.
          setOverview(stage.overview);
          setDevices(stage.devices);
        }
      }),
      listen<DiscoveryReport>("discovery://complete", (event) => {
        setReport(event.payload);
        setOverview(event.payload.overview);
        setDevices(event.payload.devices);
        setSources(event.payload.quality.sources);
      }),
      listen<string>("discovery://failed", (event) => setFailure(event.payload)),
    ];

    void invoke("start_discovery");
    return () => {
      void Promise.all(unlisteners).then((fns) => fns.forEach((fn) => fn()));
    };
  }, []);

  const openGroup = useCallback(async (category: Category, page = 0) => {
    setOpenCategory(category);
    setGroup(await invoke<GroupView>("group_view", { category, page }));
  }, []);

  const closeGroup = useCallback(() => {
    setOpenCategory(null);
    setGroup(null);
  }, []);

  // Search is local: the device list is already in hand, so no network work.
  const matched = useMemo(() => searchDevices(devices, query), [devices, query]);
  const highlighted = useMemo(
    () => new Set(matched.map((d) => d.id)),
    [matched],
  );

  const selectedDevice = useMemo(
    () => devices.find((d) => d.id === selected) ?? null,
    [devices, selected],
  );

  const onSelect = useCallback((node: TopologyNode) => setSelected(node.device_id), []);

  if (failure) {
    return (
      <section className="topo-shell">
        <div className="banner warn">
          <strong>Discovery could not start.</strong>
          <div className="note">{failure}</div>
        </div>
      </section>
    );
  }

  return (
    <section className="topo-shell">
      <div className="topo-head">
        <div>
          <h3>Your network</h3>
          <p className="note">
            {overview
              ? `${overview.total} devices · router at the centre · this Mac highlighted`
              : "Reading the neighbour cache…"}
          </p>
        </div>
        <input
          className="search"
          type="search"
          placeholder="Search name, address, vendor…"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          aria-label="Search devices by observed facts"
        />
      </div>

      {query && (
        <div className="note search-result">
          {matched.length} of {devices.length} devices match. Searching observed
          facts only — JRX has no owner information to search.
        </div>
      )}

      {report && <QualityBanner quality={report.quality} />}

      <div className="topo-body">
        {overview ? (
          <TopologyView
            overview={overview}
            group={group}
            openCategory={openCategory}
            highlighted={highlighted}
            searching={query.trim().length > 0}
            onOpenGroup={(c) => void openGroup(c)}
            onCloseGroup={closeGroup}
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
            <DiscoveryProgress sources={sources} done={report !== null} />
          )}
        </div>
      </div>
    </section>
  );
}
