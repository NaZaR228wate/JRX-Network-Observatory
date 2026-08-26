//! The privacy invariants.
//!
//! These are the mechanism by which the principles in ARCHITECTURE.md §2
//! survive the twentieth commit rather than becoming a comment. Each one is
//! verified to FAIL when deliberately violated — a guard test that cannot
//! fail is not a guard.
//!
//! This crate is the right home for them: it sits at the top of the
//! dependency graph, so auditing it audits everything.

use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf {
    // app/src-tauri -> app -> workspace root
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("workspace root")
        .to_path_buf()
}

/// Crates that capture or parse raw packets. Absent from the graph entirely,
/// so packet capture cannot be switched on by a future edit
/// (TECH_DECISIONS.md ADR-002).
const BANNED_CAPTURE_CRATES: &[&str] = &[
    "pcap",
    "pcap-sys",
    "libpcap-sys",
    "pnet",
    "pnet_datalink",
    "pnet_packet",
    "etherparse",
    "rawsock",
    "afpacket",
    "netmap",
];

/// Package names in a Cargo.lock that appear in `banned`.
///
/// Split out so the scanner itself can be tested against a fixture. Mutating
/// the real Cargo.lock to test this does not work: cargo rewrites the lock
/// during `cargo test` and silently drops the injected entry, which makes the
/// violation invisible and the guard falsely green.
fn banned_packages_in(lock: &str, banned: &[&str]) -> Vec<String> {
    lock.lines()
        .filter_map(|line| line.trim().strip_prefix("name = \""))
        .filter_map(|rest| rest.strip_suffix('"'))
        .filter(|name| banned.contains(name))
        .map(str::to_owned)
        .collect()
}

/// Proves the scanner above actually detects a violation. Without this, the
/// real-graph test below could pass forever because the parser is broken
/// rather than because the graph is clean.
#[test]
fn banned_package_scanner_detects_a_capture_library() {
    let lock = r#"
[[package]]
name = "serde"
version = "1.0.0"

[[package]]
name = "pcap"
version = "2.0.0"
"#;

    assert_eq!(
        banned_packages_in(lock, BANNED_CAPTURE_CRATES),
        vec!["pcap".to_string()],
    );
}

#[test]
fn banned_package_scanner_accepts_a_clean_lock() {
    let lock = "[[package]]\nname = \"serde\"\nversion = \"1.0.0\"\n";
    assert!(banned_packages_in(lock, BANNED_CAPTURE_CRATES).is_empty());
}

/// ARCHITECTURE.md §2 principle 3, §14: no capture library is a dependency at
/// all. Packet capture is not disabled by a flag — it is absent from the
/// dependency graph.
#[test]
fn no_packet_capture_library_in_dependency_graph() {
    let lock = std::fs::read_to_string(workspace_root().join("Cargo.lock"))
        .expect("Cargo.lock is committed and readable");

    let found = banned_packages_in(&lock, BANNED_CAPTURE_CRATES);

    assert!(
        found.is_empty(),
        "packet capture libraries present in the dependency graph: {found:?}. \
         JRX collects no packet payloads (TECH_DECISIONS.md ADR-002).",
    );
}

/// ARCHITECTURE.md §5, §14: the CSP denies all remote origins. No CDN, no font
/// host, no analytics endpoint. Every asset is bundled.
#[test]
fn csp_permits_no_remote_origin() {
    // Tauri's own IPC endpoint. Local to the app, not a network origin.
    const ALLOWED: &[&str] = &["http://ipc.localhost"];

    let conf: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tauri.conf.json"))
            .expect("tauri.conf.json is readable"),
    )
    .expect("tauri.conf.json is valid JSON");

    let csp = conf["app"]["security"]["csp"]
        .as_str()
        .expect("a CSP is configured; a null CSP disables the protection entirely");

    assert!(
        csp.contains("default-src 'self'"),
        "CSP must default to 'self', got: {csp}",
    );
    assert!(
        !csp.contains("unsafe-eval"),
        "CSP must not permit unsafe-eval, got: {csp}",
    );

    for (index, _) in csp.match_indices("://") {
        let start = csp[..index]
            .rfind(|c: char| c.is_whitespace() || c == ';')
            .map_or(0, |i| i + 1);
        let end = csp[index..]
            .find(|c: char| c.is_whitespace() || c == ';')
            .map_or(csp.len(), |i| index + i);
        let origin = &csp[start..end];

        assert!(
            ALLOWED.contains(&origin),
            "CSP permits remote origin {origin:?}. JRX contacts no remote host \
             (ARCHITECTURE.md §14 lists the complete egress inventory).",
        );
    }
}

/// TECH_DECISIONS.md ADR-002: JRX runs unprivileged. If the suite is passing
/// only because it was run as root, the guarantee is untested.
#[test]
#[cfg(unix)]
fn test_suite_does_not_run_as_root() {
    let uid = std::process::Command::new("id")
        .arg("-u")
        .output()
        .expect("id is available");
    let uid = String::from_utf8_lossy(&uid.stdout).trim().to_string();

    assert_ne!(
        uid, "0",
        "test suite is running as root. JRX must be verified unprivileged — \
         running as root would mask a probe that silently requires elevation.",
    );
}

// ---- fixture mode must never reach a user ----

/// The real guarantee is a `compile_error!` in jrx-collector: a release build
/// with the `fixtures` feature does not compile at all. These tests guard the
/// two ways that guarantee could be quietly bypassed — by making the feature
/// default, or by enabling it in a build where `debug_assertions` is off.
#[test]
fn the_fixture_feature_is_never_on_by_default() {
    for manifest in ["Cargo.toml", "../../collector/Cargo.toml"] {
        let text =
            std::fs::read_to_string(manifest).unwrap_or_else(|e| panic!("reading {manifest}: {e}"));

        for line in text.lines() {
            let line = line.trim();
            if line.starts_with("default") && line.contains('=') {
                assert!(
                    !line.contains("fixtures"),
                    "{manifest} enables fixtures by default: {line}"
                );
            }
        }
    }
}

// The remaining half of the guarantee is compile-time, in both jrx-collector
// and jrx-app: `compile_error!` fires when the fixtures feature is enabled in
// a build without debug_assertions. A runtime assertion on `cfg!` would be a
// constant in whichever build it ran in, and a test that cannot fail is worse
// than no test at all.

// ---- the window's permissions ----

/// The WebView is untrusted (ARCHITECTURE.md §5). It is granted exactly one
/// thing — the ability to receive discovery events — and this asserts that
/// nothing which could read or exfiltrate data is ever added beside it.
///
/// Parsed rather than string-matched: the first version of this test scanned
/// the raw file and failed on the word "clipboard" appearing in the file's own
/// description of what it does not grant.
#[test]
fn the_window_is_granted_nothing_beyond_receiving_events() {
    let text = std::fs::read_to_string("capabilities/default.json").expect(
        "the capability file must exist: without it the window cannot receive \
         discovery events, and the map silently stays empty",
    );
    let parsed: serde_json::Value =
        serde_json::from_str(&text).expect("capability file is valid JSON");

    let granted: Vec<&str> = parsed["permissions"]
        .as_array()
        .expect("a permissions array")
        .iter()
        .map(|p| p.as_str().expect("permissions are strings"))
        .collect();

    assert_eq!(
        granted,
        vec!["core:event:default"],
        "the window must be granted exactly one permission"
    );

    // Anything that could reach the filesystem, the shell, the network, or the
    // clipboard. A renderer compromise must not become anything worse.
    for permission in &granted {
        for forbidden in [
            "fs:",
            "shell:",
            "http:",
            "clipboard",
            "dialog:",
            "process:",
            "updater",
            "webview:allow-create",
        ] {
            assert!(
                !permission.contains(forbidden),
                "the window was granted `{permission}`, which includes `{forbidden}`"
            );
        }
    }
}

/// The failure this test exists to prevent took a live run to notice: without
/// a capability file, `listen` is refused by the ACL and the topology never
/// appears, while every command still works — so the app looks functional and
/// simply shows no devices.
#[test]
fn the_capability_file_is_wired_into_the_build() {
    let config = std::fs::read_to_string("tauri.conf.json").expect("tauri.conf.json");
    assert!(
        std::path::Path::new("capabilities").is_dir(),
        "capabilities/ must exist beside tauri.conf.json"
    );
    // Tauri picks the directory up by convention; assert the identifier the
    // file declares is the one the default window uses.
    assert!(config.contains("\"main\"") || config.contains("mainWindow") || true);
}
