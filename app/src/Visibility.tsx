import { useState } from "react";
import type { CapabilityMatrix } from "./types";
import { VisibilityPanel } from "./VisibilityPanel";

/** What JRX can and cannot see, in plain language.
 *
 *  The full capability matrix is still generated from what each probe declares
 *  and is one click away. What changed here is only the wording: the honesty
 *  model underneath is untouched. */
export function Visibility({ matrix }: { matrix: CapabilityMatrix }) {
  const [detailed, setDetailed] = useState(false);

  const count = (state: string) =>
    matrix.rows.filter((r) => r.state.state === state).length;
  const blocked = count("not_possible") + matrix.limitations.length;

  return (
    <section className="visibility">
      <h3>What JRX can see</h3>

      <div className="vis-summary">
        <div className="vis-col can">
          <h4>JRX can see</h4>
          <ul>
            <li>devices your computer already knows about</li>
            <li>names and services devices announce on this network</li>
            <li>your own network connection and how much data it is moving</li>
          </ul>
        </div>

        <div className="vis-col cannot">
          <h4>JRX cannot see</h4>
          <ul>
            <li>the contents of anything you or anyone else sends</li>
            <li>passwords, messages, or which websites anyone visits</li>
            <li>what other devices are doing in detail</li>
            <li>devices a network deliberately keeps apart from each other</li>
          </ul>
        </div>
      </div>

      <div className="vis-counts">
        <span className="pill ok">{count("observed")} working now</span>
        <span className="pill warn">{count("available")} need permission</span>
        <span className="pill off">{blocked} not possible here</span>
        <span className="pill refuse">{matrix.refused.length} refused by design</span>
      </div>

      <p className="note vis-distinction">
        <strong>Not possible here</strong> means the operating system will not
        allow it without administrator access, which JRX does not ask for.{" "}
        <strong>Refused by design</strong> means JRX could collect it and
        deliberately does not.
      </p>

      <button className="disclose" onClick={() => setDetailed(!detailed)}>
        {detailed ? "Hide the full breakdown" : "Show the full breakdown"}
      </button>

      {detailed && <VisibilityPanel matrix={matrix} />}
    </section>
  );
}
