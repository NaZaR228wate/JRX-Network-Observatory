import type { ConnectionType, NetworkIdentity, WifiStatus } from "./types";

export function connectionLabel(c: ConnectionType): string {
  switch (c) {
    case "wifi": return "Wi-Fi";
    case "ethernet": return "Ethernet (wired)";
    case "usb_tether": return "Phone hotspot over USB";
    case "unknown": return "Unknown connection";
  }
}

export function bandLabel(b: string | null): string | null {
  if (b === "ghz2_4") return "2.4 GHz";
  if (b === "ghz5") return "5 GHz";
  if (b === "ghz6") return "6 GHz";
  return null;
}

/** What to show on the "Network" row. Each Wi-Fi state gets its own sentence
 *  rather than one empty result (ARCHITECTURE.md §12). */
export function networkLine(id: NetworkIdentity): {
  value: string;
  note?: string;
  tone?: "ok" | "warn" | "off";
} {
  const w: WifiStatus = id.wifi;

  if (w.status === "associated" && w.ssid) {
    const bits = [
      bandLabel(w.band),
      w.channel != null ? `channel ${w.channel}` : null,
      w.signal_dbm != null ? `${w.signal_dbm} dBm` : null,
      w.security,
    ].filter(Boolean);
    return { value: w.ssid, note: bits.join(" · ") || undefined, tone: "ok" };
  }

  if (w.status === "permission_required") {
    return {
      value: "Network name withheld",
      note: "macOS requires Location Services to reveal the Wi-Fi network name. Nothing else is affected.",
      tone: "warn",
    };
  }

  if (w.status === "unavailable") {
    return {
      value: "Wi-Fi could not be read",
      note: `The Wi-Fi probe failed: ${w.reason}. This is a fault in JRX, not a fact about your Mac.`,
      tone: "warn",
    };
  }

  // A phone sharing its connection is not a network with a name, and the
  // state of this machine's Wi-Fi radio has nothing to do with it.
  if (id.connection === "usb_tether") {
    return {
      value: "Shared from a phone",
      note: "Your phone is passing its own connection through to this computer. Networks like this are usually just the two of you.",
      tone: "off",
    };
  }

  if (id.connection === "ethernet") {
    const note =
      w.status === "radio_off"
        ? "A cable does not carry a network name. Wi-Fi is switched off on this machine."
        : w.status === "no_hardware"
          ? "A cable does not carry a network name, and this machine has no Wi-Fi."
          : "A cable does not carry a network name.";
    return { value: "Wired connection", note, tone: "off" };
  }

  if (w.status === "radio_off") {
    return { value: "Wi-Fi is switched off", tone: "off" };
  }
  if (w.status === "no_hardware") {
    return { value: "No Wi-Fi hardware", tone: "off" };
  }
  return { value: "Not joined to a Wi-Fi network", tone: "off" };
}
