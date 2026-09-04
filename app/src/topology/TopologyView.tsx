import { useMemo, useState } from "react";
import type {
  Category,
  CategorySummary,
  Device,
  GroupView,
  TopologyNode,
  TopologyOverview,
} from "../types";
import { FILTER_CHOICES } from "../types";
import { categoryTone } from "./visual";
import { NodeGlyph } from "./icons";
import { nodeRadius, placeGroups, placeMembers, placeSelf } from "./layout";

const WIDTH = 760;
const HEIGHT = 470;
const CENTER = { x: WIDTH / 2, y: HEIGHT / 2 };
const RING = 176;

/** Below this many devices, the map shows every device individually with its
 *  own icon, the way a person pictures a home network. Above it, devices are
 *  grouped by kind so the picture stays legible — the same reason a phone book
 *  is not one long list of everyone's number. */
const FLAT_MAX = 14;

interface Props {
  overview: TopologyOverview;
  devices?: Device[];
  group: GroupView | null;
  openCategory: Category | null;
  highlighted: Set<string>;
  searching: boolean;
  selectedId?: string | null;
  onOpenGroup: (category: Category) => void;
  onCloseGroup: () => void;
  onSelectDevice: (node: TopologyNode) => void;
  onPage: (page: number) => void;
  filterKey?: string;
  onFilter?: (key: string) => void;
  /** Devices new on this network since last time (empty unless returning). */
  newDevices?: Set<string>;
}

export function TopologyView(props: Props) {
  const flat =
    !props.openCategory &&
    props.devices !== undefined &&
    props.overview.total <= FLAT_MAX;

  return (
    <div className="topo">
      <svg
        viewBox={`0 0 ${WIDTH} ${HEIGHT}`}
        className="topo-svg"
        role="img"
        aria-label={
          props.openCategory
            ? `${props.group?.label ?? ""} devices`
            : "Network overview, router at centre"
        }
      >
        <defs>
          <radialGradient id="anchor" cx="50%" cy="50%">
            <stop offset="0%" stopColor="rgba(76,194,255,0.20)" />
            <stop offset="100%" stopColor="rgba(76,194,255,0)" />
          </radialGradient>
        </defs>
        <circle cx={CENTER.x} cy={CENTER.y} r={RING * 1.15} fill="url(#anchor)" />

        {props.openCategory && props.group ? (
          <GroupLevel {...props} group={props.group} />
        ) : flat ? (
          <FlatLevel {...props} devices={props.devices!} />
        ) : (
          <OverviewLevel {...props} />
        )}
      </svg>

      {props.openCategory && props.group && (
        <GroupChrome
          group={props.group}
          filterKey={props.filterKey ?? "all"}
          onFilter={props.onFilter}
          onClose={props.onCloseGroup}
          onPage={props.onPage}
        />
      )}
    </div>
  );
}

/** Device → node. Everything a node needs is already in the device. */
function deviceToNode(d: Device): TopologyNode {
  return {
    device_id: d.id,
    display_name: displayName(d),
    category: d.inference.category,
    confidence: d.inference.confidence,
    family: d.inference.family,
    rationale: d.inference.rationale,
    evidence: d.inference.supporting,
    vendor: d.facts.vendor,
    mac_randomised: d.facts.mac_randomised,
    sources: d.facts.sources,
    is_self: d.is_self,
    is_gateway: d.is_gateway,
  };
}

function displayName(d: Device): string {
  if (d.facts.hostname) return d.facts.hostname;
  if (d.facts.vendor) return `${d.facts.vendor} device`;
  return d.facts.addresses[0] ?? "Unidentified device";
}

// ---------- flat overview (small networks) ----------

function FlatLevel({
  overview,
  devices,
  onSelectDevice,
  highlighted,
  searching,
  selectedId,
  newDevices,
}: Props & { devices: Device[] }) {
  const others = useMemo(
    () => devices.filter((d) => !d.is_self && !d.is_gateway).map(deviceToNode),
    [devices],
  );
  const points = useMemo(
    () => placeMembers(CENTER, others.length, RING * 0.92, 66),
    [others.length],
  );
  const self = useMemo(() => placeSelf(CENTER, RING), []);

  return (
    <g>
      {points.map((point, index) => (
        <line
          key={`spoke-${others[index]!.device_id}`}
          x1={CENTER.x}
          y1={CENTER.y}
          x2={point.x}
          y2={point.y}
          className="spoke faint"
        />
      ))}
      {overview.self_node && (
        <line x1={CENTER.x} y1={CENTER.y} x2={self.x} y2={self.y} className="spoke self" />
      )}

      {others.map((node, index) => {
        const point = points[index];
        if (!point) return null;
        const dimmed = searching && !highlighted.has(node.device_id);
        return (
          <g key={node.device_id} className={dimmed ? "dimmed" : undefined}>
            <DeviceNode
              node={node}
              x={point.x}
              y={point.y}
              radius={nodeRadius("member")}
              label={node.display_name}
              emphasis="device"
              alwaysLabel
              selected={node.device_id === selectedId}
              isNew={newDevices?.has(node.device_id) ?? false}
              onSelect={() => onSelectDevice(node)}
            />
          </g>
        );
      })}

      {overview.self_node && (
        <DeviceNode
          node={overview.self_node}
          x={self.x}
          y={self.y}
          radius={nodeRadius("self")}
          label="This Mac"
          emphasis="self"
          alwaysLabel
          selected={overview.self_node.device_id === selectedId}
          onSelect={() => onSelectDevice(overview.self_node!)}
        />
      )}

      {overview.center && (
        <DeviceNode
          node={overview.center}
          x={CENTER.x}
          y={CENTER.y}
          radius={nodeRadius("router")}
          label={overview.center.display_name}
          emphasis="router"
          alwaysLabel
          selected={overview.center.device_id === selectedId}
          onSelect={() => onSelectDevice(overview.center!)}
        />
      )}
    </g>
  );
}

// ---------- level 1: grouped overview (large networks) ----------

function OverviewLevel({ overview, onOpenGroup, onSelectDevice, selectedId }: Props) {
  const placed = useMemo(() => placeGroups(CENTER, RING), []);
  const self = useMemo(() => placeSelf(CENTER, RING), []);

  return (
    <g>
      {placed.map((spot) => (
        <line
          key={`spoke-${spot.category}`}
          x1={CENTER.x}
          y1={CENTER.y}
          x2={spot.x}
          y2={spot.y}
          className="spoke"
        />
      ))}
      <line x1={CENTER.x} y1={CENTER.y} x2={self.x} y2={self.y} className="spoke self" />

      {placed.map((spot) => {
        const group = overview.groups.find((g) => g.category === spot.category);
        if (!group) return null;

        // The router is drawn in the centre, so its own category's ring is
        // empty. Showing a bare "0" there reads as broken when in fact the
        // one device is right in the middle of the picture.
        const routerOnly =
          group.category === "infrastructure" &&
          group.count === 0 &&
          overview.center !== null;

        return (
          <GroupNode
            key={spot.category}
            x={spot.x}
            y={spot.y}
            group={group}
            routerOnly={routerOnly}
            onOpen={() => {
              if (routerOnly && overview.center) onSelectDevice(overview.center);
              else if (group.count > 0) onOpenGroup(group.category);
            }}
          />
        );
      })}

      {overview.self_node && (
        <DeviceNode
          node={overview.self_node}
          x={self.x}
          y={self.y}
          radius={nodeRadius("self")}
          label="This Mac"
          emphasis="self"
          selected={overview.self_node.device_id === selectedId}
          onSelect={() => onSelectDevice(overview.self_node!)}
        />
      )}

      {overview.center && (
        <DeviceNode
          node={overview.center}
          x={CENTER.x}
          y={CENTER.y}
          radius={nodeRadius("router")}
          label={overview.center.display_name}
          emphasis="router"
          selected={overview.center.device_id === selectedId}
          onSelect={() => onSelectDevice(overview.center!)}
        />
      )}
    </g>
  );
}

function GroupNode({
  x,
  y,
  group,
  routerOnly,
  onOpen,
}: {
  x: number;
  y: number;
  group: CategorySummary;
  routerOnly: boolean;
  onOpen: () => void;
}) {
  const interactive = group.count > 0 || routerOnly;
  const r = nodeRadius("group");
  const count = routerOnly ? 1 : group.count;
  const badgeW = Math.max(20, String(count).length * 8 + 10);

  return (
    <g
      className={`node group ${categoryTone(group.category)} ${interactive ? "" : "empty"}`}
      transform={`translate(${x} ${y})`}
      role={interactive ? "button" : undefined}
      tabIndex={interactive ? 0 : undefined}
      aria-label={
        routerOnly
          ? "Infrastructure: 1 device, your router, shown in the centre"
          : `${group.label}, ${group.count} devices`
      }
      onClick={onOpen}
      onKeyDown={(e) => (e.key === "Enter" || e.key === " ") && onOpen()}
    >
      <circle r={r} className="node-body" />
      <NodeGlyph category={group.category} size={r * 1.15} />
      {count > 0 && (
        <g className="node-badge" transform={`translate(${r * 0.72} ${-r * 0.72})`}>
          <rect x={-badgeW / 2} y={-9} width={badgeW} height={18} rx={9} />
          <text dy="0.34em">{count}</text>
        </g>
      )}
      <text className="node-label" y={r + 18}>
        {group.label}
      </text>
      {routerOnly && (
        <text className="node-sub" y={r + 33}>
          router, in centre
        </text>
      )}
    </g>
  );
}

// ---------- level 2: members of one category ----------

const LABEL_BUDGET = 12;

function GroupLevel({
  group,
  onSelectDevice,
  highlighted,
  searching,
  selectedId,
  newDevices,
}: Props & { group: GroupView }) {
  const points = useMemo(
    () => placeMembers(CENTER, group.devices.length, 116, 66),
    [group.devices.length],
  );
  // Naming a handful of nodes is informative; naming a hundred is noise.
  const alwaysLabel = group.devices.length <= LABEL_BUDGET;

  return (
    <g>
      <circle cx={CENTER.x} cy={CENTER.y} r={nodeRadius("group")} className="group-anchor" />
      <text x={CENTER.x} y={CENTER.y} className="node-count" dy="0.35em">
        {group.total}
      </text>
      <text x={CENTER.x} y={CENTER.y + 44} className="node-label">
        {group.label}
      </text>

      {group.devices.map((node, index) => {
        const point = points[index];
        if (!point) return null;
        const dimmed = searching && !highlighted.has(node.device_id);
        return (
          <g key={node.device_id} className={dimmed ? "dimmed" : undefined}>
            <line
              x1={CENTER.x}
              y1={CENTER.y}
              x2={point.x}
              y2={point.y}
              className="spoke faint"
            />
            <DeviceNode
              node={node}
              x={point.x}
              y={point.y}
              radius={nodeRadius("member")}
              label={node.display_name}
              emphasis={node.is_self ? "self" : "device"}
              alwaysLabel={alwaysLabel}
              selected={node.device_id === selectedId}
              isNew={newDevices?.has(node.device_id) ?? false}
              onSelect={() => onSelectDevice(node)}
            />
          </g>
        );
      })}
    </g>
  );
}

function GroupChrome({
  group,
  filterKey,
  onFilter,
  onClose,
  onPage,
}: {
  group: GroupView;
  filterKey: string;
  onFilter?: (key: string) => void;
  onClose: () => void;
  onPage: (page: number) => void;
}) {
  return (
    <div className="group-chrome">
      <div className="chrome-top">
        <button className="back" onClick={onClose}>
          ← Whole network
        </button>
        {onFilter && (
          <div className="filters" role="group" aria-label="Narrow by observed facts">
            {FILTER_CHOICES.map((choice) => (
              <button
                key={choice.key}
                className={choice.key === filterKey ? "on" : undefined}
                onClick={() => onFilter(choice.key)}
              >
                {choice.label}
              </button>
            ))}
          </div>
        )}
      </div>

      {group.facts.length > 0 && (
        <div className="group-facts">
          <div className="note">
            {group.total} devices JRX did not identify. These counts are
            separate observations and overlap — they are facts about the
            devices, not categories.
          </div>
          <ul>
            {group.facts.map((fact) => (
              <li key={fact.description}>
                <strong>{fact.count}</strong> {fact.description}
              </li>
            ))}
          </ul>
        </div>
      )}

      {group.page_count > 1 && (
        <div className="pager">
          <button disabled={group.page === 0} onClick={() => onPage(group.page - 1)}>
            ←
          </button>
          <span className="mono">
            {group.page * group.page_size + 1}–
            {Math.min((group.page + 1) * group.page_size, group.total)} of {group.total}
          </span>
          <button
            disabled={group.page >= group.page_count - 1}
            onClick={() => onPage(group.page + 1)}
          >
            →
          </button>
        </div>
      )}
    </div>
  );
}

// ---------- shared ----------

function DeviceNode({
  node,
  x,
  y,
  radius,
  label,
  emphasis,
  alwaysLabel = false,
  isNew = false,
  selected = false,
  onSelect,
}: {
  node: TopologyNode;
  x: number;
  y: number;
  radius: number;
  label: string;
  emphasis: "router" | "self" | "device";
  alwaysLabel?: boolean;
  isNew?: boolean;
  selected?: boolean;
  onSelect: () => void;
}) {
  const [hover, setHover] = useState(false);
  const showLabel = emphasis !== "device" || alwaysLabel || hover;
  const glyphCategory: Category =
    emphasis === "router"
      ? "infrastructure"
      : emphasis === "self"
        ? "computers"
        : node.category;

  return (
    <g
      className={`node ${emphasis} ${categoryTone(node.category)} conf-${node.confidence}${selected ? " active" : ""}`}
      transform={`translate(${x} ${y})`}
      role="button"
      tabIndex={0}
      aria-label={`${label}. ${node.rationale}${isNew ? ". New on this network since last time" : ""}`}
      onClick={onSelect}
      onKeyDown={(e) => (e.key === "Enter" || e.key === " ") && onSelect()}
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => setHover(false)}
    >
      {selected && <circle r={radius + 6} className="select-ring" />}
      {emphasis === "self" && <circle r={radius + 7} className="halo" />}
      {isNew && <circle r={radius + 4} className="new-ring" />}
      <circle r={radius} className="node-body" />
      <NodeGlyph category={glyphCategory} size={radius * 1.15} />
      {emphasis === "device" && node.confidence !== "none" && (
        <circle className="conf-dot" r={2.6} cx={radius * 0.62} cy={-radius * 0.62} />
      )}
      {showLabel && (
        <text className="node-label" y={radius + 15}>
          {label.length > 22 ? `${label.slice(0, 21)}…` : label}
        </text>
      )}
    </g>
  );
}
