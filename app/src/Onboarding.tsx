import { useCallback, useEffect, useState } from "react";

const SEEN_KEY = "jrx.onboarded.v1";

/** True on a first launch (nothing remembered yet). `?onboard=1` forces it,
 *  so the development preview can review the panel. */
function firstRun(): boolean {
  try {
    if (new URLSearchParams(window.location.search).get("onboard") === "1") {
      return true;
    }
    return window.localStorage.getItem(SEEN_KEY) !== "1";
  } catch {
    // Private windows and locked-down webviews throw on access. If it can't be
    // remembered, don't show it — nagging every launch is worse than skipping.
    return false;
  }
}

/** A one-time welcome that states, before anything else, what JRX will never
 *  do. Shown on first launch, remembered locally, never network-bound. */
export function Onboarding() {
  const [open, setOpen] = useState(firstRun);

  const dismiss = useCallback(() => {
    try {
      window.localStorage.setItem(SEEN_KEY, "1");
    } catch {
      // Closing it for this launch is enough when it can't be persisted.
    }
    setOpen(false);
  }, []);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") dismiss();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, dismiss]);

  if (!open) return null;

  return (
    <div
      className="onboard"
      role="dialog"
      aria-modal="true"
      aria-labelledby="onboard-title"
      onClick={dismiss}
    >
      <div className="onboard-card" onClick={(e) => e.stopPropagation()}>
        <div className="brand">
          <h1>JRX</h1>
          <span className="tag">Network Observatory</span>
        </div>

        <h2 id="onboard-title" className="onboard-title">
          A map of your network that admits what it doesn’t know.
        </h2>

        <p className="onboard-lead">
          JRX shows what you’re connected to, who else is on the network, and
          what this Mac is doing on it — always separating what it{" "}
          <em>observed</em> from what it <em>inferred</em>. When the evidence is
          thin it says <strong>unidentified</strong>, rather than guessing.
        </p>

        <h3 className="onboard-heading">What it will never do</h3>
        <ul className="onboard-never">
          <li>Capture packets, messages, or credentials</li>
          <li>Peer inside your encrypted TLS traffic, or log your browsing and DNS</li>
          <li>Turn a network owner into a website you visited</li>
          <li>Phone home, use an account, or ask for admin access</li>
        </ul>

        <p className="onboard-foot">
          Everything runs here, on this Mac. Nothing leaves it.
        </p>

        <button className="onboard-go" onClick={dismiss} autoFocus>
          Show me my network
        </button>
      </div>
    </div>
  );
}
