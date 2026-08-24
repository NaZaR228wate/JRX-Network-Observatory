# metrics/

**Local development and performance measurements only.**

This directory holds build-time and development-time measurement helpers —
collector probe timings, cold-start latency budgets (see
[ARCHITECTURE.md §7.1](../ARCHITECTURE.md)), and rendering performance
benchmarks used while developing JRX.

## What this directory is not

Despite what the word "metrics" implies in most products, nothing here reports
anything anywhere.

**No user data, network inventory, device information, or telemetry of any kind
leaves the device.** JRX has no backend, no analytics, no crash reporting, no
update ping, and no license check. The complete inventory of what the application
is permitted to send is enumerated in [ARCHITECTURE.md §14](../ARCHITECTURE.md),
and it contains five items — all of them LAN-local or explicitly user-initiated.

This directory was originally scaffolded as `telemetry/`. It was renamed because
"telemetry" reads as phone-home in a privacy-first security product, and it is
among the first words a skeptical user or reviewer will search for. The name
should describe what the code does. See
[TECH_DECISIONS.md ADR-014](../TECH_DECISIONS.md).
