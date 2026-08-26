import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { Screen } from "./Screen";
import type { ScreenData } from "./Screen";
import type {
  CapabilityMatrix,
  Category,
  Device,
  DiscoveryReport,
  DiscoveryStage,
  GroupView,
  NetworkIdentityReport,
  SourceQuality,
  TopologyOverview,
} from "./types";
import { FILTER_CHOICES } from "./types";
import "./styles.css";

/** Live wiring. All rendering lives in Screen, which the development preview
 *  drives with fixture data — so what is reviewed visually is what ships. */
export function App() {
  const [identity, setIdentity] = useState<NetworkIdentityReport | null>(null);
  const [capabilities, setCapabilities] = useState<CapabilityMatrix | null>(null);
  const [overview, setOverview] = useState<TopologyOverview | null>(null);
  const [devices, setDevices] = useState<Device[]>([]);
  const [report, setReport] = useState<DiscoveryReport | null>(null);
  const [sources, setSources] = useState<SourceQuality[]>([]);
  const [failure, setFailure] = useState<string | null>(null);

  useEffect(() => {
    let disposed = false;
    let unlisteners: UnlistenFn[] = [];

    // The identity resolves in a few hundred milliseconds, so the first thing
    // on screen is the answer to "what am I connected to?".
    invoke<NetworkIdentityReport>("get_network_identity")
      .then(setIdentity)
      .catch((e: unknown) => setFailure(String(e)));
    invoke<CapabilityMatrix>("get_capabilities").then(setCapabilities).catch(() => undefined);

    const pending = [
      listen<DiscoveryStage>("discovery://stage", (event) => {
        const stage = event.payload;
        if (stage.stage === "source_finished") {
          setSources((prev) =>
            prev.some((s) => s.method === stage.source.method)
              ? prev
              : [...prev, stage.source],
          );
        } else if (stage.stage === "partial") {
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

    // Discovery must not start until the listeners are actually registered.
    // The neighbour cache reports within milliseconds, and `listen` resolves
    // asynchronously: starting first loses that first result, and with it the
    // map's entire reason for appearing quickly.
    Promise.all(pending)
      .then((fns) => {
        unlisteners = fns;
        if (disposed) {
          fns.forEach((fn) => fn());
          return;
        }
        return invoke("start_discovery");
      })
      // Silence here would leave the map empty with no explanation, which is
      // the one thing this product must never do.
      .catch((e: unknown) => setFailure(`discovery could not start: ${String(e)}`));

    return () => {
      disposed = true;
      unlisteners.forEach((fn) => fn());
    };
  }, []);

  const live: ScreenData = {
    identity,
    capabilities,
    overview,
    devices,
    report,
    sources,
    failure,
    // Derived by the host from the report it already holds: no network work.
    getGroup: (category: Category, page: number, filterKey: string) =>
      invoke<GroupView>("group_view", {
        category,
        page,
        filter: FILTER_CHOICES.find((c) => c.key === filterKey)?.filter ?? {},
      }),
  };

  return <Screen live={live} />;
}
