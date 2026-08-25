import { useMemo, useState } from "react";
import type {
  Category,
  CategorySummary,
  GroupView,
  TopologyNode,
  TopologyOverview,
} from "../types";
import { categoryGlyph, categoryTone } from "./visual";
import { nodeRadius, placeGroups, placeMembers, placeSelf } from "./layout";

const WIDTH = 720;
const HEIGHT = 560;
const CENTER = { x: WIDTH / 2, y: HEIGHT / 2 };
const RING = 205;

interface Props {
  overview: TopologyOverview;
  group: GroupView | null;
  openCategory: Category | null;
  highlighted: Set<string>;
  searching: boolean;
  onOpenGroup: (category: Category) => void;
  onCloseGroup: () => void;
  onSelectDevice: (node: TopologyNode) => void;
  onPage: (page: number) => void;
}

export function TopologyView(props: Props) {
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
        ) : (
          <OverviewLevel {...props} />
        )}
      </svg>

      {props.openCategory && props.group && (
        <GroupChrome
          group={props.group}
          onClose={props.onCloseGroup}
          onPage={props.onPage}
        />
      )}
    </div>
  );
}

// ---------- level 1 ----------

function OverviewLevel({
  overview,
  onOpenGroup,
  onSelectDevice,
}: Props) {
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
        return (
          <GroupNode
            key={spot.category}
            x={spot.x}
            y={spot.y}
            group={group}
            onOpen={() => group.count > 0 && onOpenGroup(group.category)}
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
  onOpen,
}: {
  x: number;
  y: number;
  group: CategorySummary;
  onOpen: () => void;
}) {
  const empty = group.count === 0;
  const r = nodeRadius("group");

  return (
    <g
      className={`node group ${categoryTone(group.category)} ${empty ? "empty" : ""}`}
      transform={`translate(${x} ${y})`}
      role={empty ? undefined : "button"}
      tabIndex={empty ? undefined : 0}
      aria-label={`${group.label}, ${group.count} devices`}
      onClick={onOpen}
      onKeyDown={(e) => (e.key === "Enter" || e.key === " ") && onOpen()}
    >
      <circle r={r} className="node-body" />
      <text className="node-count" dy="0.35em">
        {group.count}
      </text>
      <text className="node-label" y={r + 18}>
        {group.label}
      </text>
      <text className="node-sub" y={r + 33}>
        {categoryGlyph(group.category)}
      </text>
    </g>
  );
}

// ---------- level 2 ----------

const LABEL_BUDGET = 12;

function GroupLevel({ group, onSelectDevice, highlighted, searching }: Props & { group: GroupView }) {
  const points = useMemo(
    () => placeMembers(CENTER, group.devices.length, 128, 74),
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
              radius={nodeRadius("device")}
              label={node.display_name}
              emphasis={node.is_self ? "self" : "device"}
              alwaysLabel={alwaysLabel}
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
  onClose,
  onPage,
}: {
  group: GroupView;
  onClose: () => void;
  onPage: (page: number) => void;
}) {
  return (
    <div className="group-chrome">
      <button className="back" onClick={onClose}>
        ← Whole network
      </button>

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
  onSelect,
}: {
  node: TopologyNode;
  x: number;
  y: number;
  radius: number;
  label: string;
  emphasis: "router" | "self" | "device";
  alwaysLabel?: boolean;
  onSelect: () => void;
}) {
  const [hover, setHover] = useState(false);
  const showLabel = emphasis !== "device" || alwaysLabel || hover;

  return (
    <g
      className={`node ${emphasis} ${categoryTone(node.category)}`}
      transform={`translate(${x} ${y})`}
      role="button"
      tabIndex={0}
      aria-label={`${label}. ${node.rationale}`}
      onClick={onSelect}
      onKeyDown={(e) => (e.key === "Enter" || e.key === " ") && onSelect()}
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => setHover(false)}
    >
      {emphasis === "self" && <circle r={radius + 7} className="halo" />}
      <circle r={radius} className="node-body" />
      {emphasis !== "device" && (
        <text className="node-sub" dy="0.35em">
          {emphasis === "router" ? "◉" : "▲"}
        </text>
      )}
      {showLabel && (
        <text className="node-label" y={radius + 15}>
          {label.length > 22 ? `${label.slice(0, 21)}…` : label}
        </text>
      )}
    </g>
  );
}
