// Development-only visual preview.
//
// Renders the real screen against the real pipeline's output, so layout can be
// reviewed at any window size and in any network mode without waiting for a
// matching network to exist.
//
// Not part of the production build: vite builds index.html only, so nothing
// here — including the fixture payloads — reaches a shipped binary.

import { useState } from "react";
import { createRoot } from "react-dom/client";
import { Screen } from "./Screen";
import type { PreviewData } from "./Screen";
import "./styles.css";

import ethernet from "../fixtures/ethernet.json";
import home from "../fixtures/home_wifi.json";
import hotspot from "../fixtures/hotspot.json";
import isolated from "../fixtures/isolated_network.json";
import permission from "../fixtures/permission_limited.json";
import university from "../fixtures/university_wifi.json";
import vpn from "../fixtures/vpn.json";

const FIXTURES: Record<string, unknown> = {
  home_wifi: home,
  university_wifi: university,
  ethernet,
  vpn,
  isolated_network: isolated,
  permission_limited: permission,
  hotspot,
};

function Preview() {
  const initial = new URLSearchParams(location.search).get("f") ?? "home_wifi";
  const [name, setName] = useState(initial);
  const data = FIXTURES[name] as PreviewData;

  return (
    <>
      <div className="preview-bar">
        {Object.keys(FIXTURES).map((key) => (
          <button
            key={key}
            className={key === name ? "on" : undefined}
            onClick={() => setName(key)}
          >
            {key}
          </button>
        ))}
      </div>
      <Screen data={data} />
    </>
  );
}

createRoot(document.getElementById("root")!).render(<Preview />);
