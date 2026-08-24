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
