# What JRX is, and what it is not

**Status:** M4.5 · **Date:** 2026-08-26

This document exists before M5 because the traffic milestone is where a
network tool is most tempted to overpromise. Everything below is a commitment,
not a description of current limitations.

---

## JRX v1 does

- **Identify the current network connection** — Wi-Fi, Ethernet, a phone
  hotspot, or honestly "unknown" when the evidence does not say. A VPN is
  reported as a route the traffic takes, never as the connection itself.
- **Discover locally observable devices** — those the operating system already
  knows about, and those that announce themselves on the network.
- **Explain what it knows and why** — every device separates what was observed
  from what was concluded, and every conclusion cites the specific observation
  behind it.
- **Classify only when the evidence supports it** — a device JRX cannot place
  stays Unidentified, which is a result rather than a failure.
- **Show the network as a map** — the router at the centre, this computer
  highlighted, everything else grouped by kind.
- **Explain its own limits** — what it can see here, what a permission would
  add, what the platform forbids, and what JRX refuses to collect.

## JRX v1 does not

- **Capture packet payloads.** There is no capture library in the dependency
  graph. This is enforced by a test, not by intention.
- **Read messages, passwords, or credentials.** Nothing in JRX asks for them.
- **Build browsing history.** JRX reads which DNS resolvers this computer is
  configured to use. It never records what was looked up.
- **Inspect the detailed traffic of other devices.** This is not a temporary
  gap. Passive observation from an ordinary machine on a switched, encrypted
  network cannot do it, and JRX will not pretend otherwise.
- **Require root or administrator access.** Not at install, not at run time,
  not for any feature.
- **Name the websites you visit.** JRX can see which of your programs are
  talking and to whose network. Turning an address into a site name would need
  it to watch your DNS lookups, which is refused above. Measured: reverse DNS
  answered for none of 12 live endpoints, and would not be proof even if it
  had.
- **Send anything anywhere.** No accounts, no backend, no analytics, no crash
  reporting, no update ping.

---

## Why the fourth one is a product decision, not a limitation

Users of network tools expect to see "what every device is doing". On a modern
network that expectation is decades out of date:

- Switched networks send each device only its own traffic.
- Nearly all of it is encrypted.
- Seeing more would require administrator access, which JRX does not take.

A tool that appeared to satisfy that expectation would be lying, guessing, or
demanding privileges it should not have. **M5 is therefore scoped as "this
computer's network activity", not "everyone else's traffic".**

---

## The 30-second product test

The acceptance criterion for the whole product. Within 30 seconds of opening
JRX, with no documentation and no explanation, a person should be able to
answer:

| # | Question | Where the answer is |
|---|---|---|
| 1 | What kind of connection am I using? | The first line of the screen |
| 2 | What network am I on? | Directly beneath it, with router and local address |
| 3 | How many devices has JRX observed? | Above the map |
| 4 | Which device is this computer? | Named in the card, highlighted on the map |
| 5 | What kinds of devices are here? | The category counts, before the map |
| 6 | What does JRX know versus infer? | Any device panel, in separate sections |
| 7 | What can JRX not see here, and why? | "What JRX can see", at the foot of the screen |

Questions 1, 2, 4 and 5 must be answerable **without scrolling** at
1280×720. Questions 3 and 7 must be answerable without clicking anything.
Question 6 may take one click.

### Wording rules

- Never report a missing permission as a missing fact. "Wi-Fi, network name
  withheld" and "unknown connection" are different statements.
- Never report our own failure as a property of the network. If a probe was
  refused, say so; do not call the network empty.
- Never present an inference as a measurement. Anything JRX worked out says so,
  in words, next to the evidence.
- Never imply visibility JRX does not have.
