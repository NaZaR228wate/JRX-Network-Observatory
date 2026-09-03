//! store.rs — local persistence for recognition (ADR-021).
//!
//! The platform adapter behind `core::history`: a small SQLite database that
//! remembers the one-way digests of the networks and devices JRX has seen, so a
//! later run can answer "have I been here before?" and "is this device new
//! here?". It stores digests only — never a name, BSSID, MAC, or address — and
//! never leaves the machine. `core` never sees this type; a future iOS client
//! reuses `core::history` and writes its own store (ARCHITECTURE.md §17).

use std::path::Path;

use jrx_core::history::{DeviceStanding, NetworkKey, NetworkRecognition, recognise_network};
use rusqlite::{Connection, OptionalExtension, params};

/// A recognition database. Wraps one SQLite connection; nothing about its
/// schema escapes this module.
pub struct RecognitionStore {
    conn: Connection,
}

pub type StoreResult<T> = Result<T, rusqlite::Error>;

impl RecognitionStore {
    /// Open (creating if absent) the database at `path`, ensuring its schema.
    pub fn open(path: impl AsRef<Path>) -> StoreResult<Self> {
        Self::init(Connection::open(path)?)
    }

    /// An in-memory database, for tests and for a run that must not persist.
    pub fn in_memory() -> StoreResult<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> StoreResult<Self> {
        conn.execute_batch(SCHEMA)?;
        Ok(Self { conn })
    }

    // --- networks ---

    pub fn network_known(&self, digest: &str) -> StoreResult<bool> {
        self.conn
            .query_row("SELECT 1 FROM networks WHERE digest = ?1", [digest], |_| {
                Ok(())
            })
            .optional()
            .map(|found| found.is_some())
    }

    pub fn network_first_seen(&self, digest: &str) -> StoreResult<Option<i64>> {
        self.conn
            .query_row(
                "SELECT first_seen FROM networks WHERE digest = ?1",
                [digest],
                |row| row.get(0),
            )
            .optional()
    }

    /// Record a sighting: insert on first sight, and on return advance
    /// `last_seen` while leaving `first_seen` as it was.
    pub fn record_network(&self, digest: &str, now: i64) -> StoreResult<()> {
        self.conn.execute(
            "INSERT INTO networks (digest, first_seen, last_seen) VALUES (?1, ?2, ?2)
             ON CONFLICT(digest) DO UPDATE SET last_seen = ?2",
            params![digest, now],
        )?;
        Ok(())
    }

    // --- devices (scoped to the network they were seen on) ---

    pub fn device_known(&self, network_digest: &str, device_digest: &str) -> StoreResult<bool> {
        self.conn
            .query_row(
                "SELECT 1 FROM devices WHERE network_digest = ?1 AND device_digest = ?2",
                [network_digest, device_digest],
                |_| Ok(()),
            )
            .optional()
            .map(|found| found.is_some())
    }

    pub fn record_device(
        &self,
        network_digest: &str,
        device_digest: &str,
        now: i64,
    ) -> StoreResult<()> {
        self.conn.execute(
            "INSERT INTO devices (network_digest, device_digest, first_seen, last_seen)
             VALUES (?1, ?2, ?3, ?3)
             ON CONFLICT(network_digest, device_digest) DO UPDATE SET last_seen = ?3",
            params![network_digest, device_digest, now],
        )?;
        Ok(())
    }

    // --- lifecycle ---

    /// Retention: forget anything not seen since `cutoff` (a Unix timestamp).
    /// Returns how many rows were dropped.
    pub fn sweep(&self, cutoff: i64) -> StoreResult<usize> {
        let networks = self
            .conn
            .execute("DELETE FROM networks WHERE last_seen < ?1", [cutoff])?;
        let devices = self
            .conn
            .execute("DELETE FROM devices WHERE last_seen < ?1", [cutoff])?;
        Ok(networks + devices)
    }

    /// Verifiable erase — the roadmap's `clear_all_data`. Empties both tables.
    pub fn clear_all(&self) -> StoreResult<()> {
        self.conn
            .execute_batch("DELETE FROM networks; DELETE FROM devices;")?;
        Ok(())
    }

    pub fn network_count(&self) -> StoreResult<i64> {
        self.conn
            .query_row("SELECT COUNT(*) FROM networks", [], |row| row.get(0))
    }

    pub fn device_count(&self) -> StoreResult<i64> {
        self.conn
            .query_row("SELECT COUNT(*) FROM devices", [], |row| row.get(0))
    }

    // --- orchestration: the small bridge between core::history and storage ---

    /// Recognise the current network and record this sighting in one step.
    /// Returns the verdict and, for a returning network, when it was first seen.
    ///
    /// The lookup happens before the write, so a first visit reads as `FirstTime`
    /// even though this same call then records it.
    pub fn observe_network(
        &self,
        key: &NetworkKey,
        now: i64,
    ) -> StoreResult<(NetworkRecognition, Option<i64>)> {
        let known = self.network_known(&key.digest)?;
        let first_seen = if known {
            self.network_first_seen(&key.digest)?
        } else {
            None
        };
        let recognition = recognise_network(key, known);
        self.record_network(&key.digest, now)?;
        Ok((recognition, first_seen))
    }

    /// The standing of a device on this network, recording the sighting.
    ///
    /// `device_key` is `None` for a device with no stable identity (a randomised
    /// or absent MAC); such a device is `CannotDetermine` and is deliberately
    /// not recorded — remembering a rotating identity would be remembering noise.
    pub fn observe_device(
        &self,
        network_digest: &str,
        device_key: Option<&str>,
        now: i64,
    ) -> StoreResult<DeviceStanding> {
        match device_key {
            None => Ok(DeviceStanding::CannotDetermine),
            Some(key) => {
                let known = self.device_known(network_digest, key)?;
                self.record_device(network_digest, key, now)?;
                Ok(if known {
                    DeviceStanding::Known
                } else {
                    DeviceStanding::New
                })
            }
        }
    }
}

/// STRICT tables reject a value of the wrong type outright, so a bug that tried
/// to store something other than a digest or a timestamp fails loudly here
/// rather than silently corrupting recognition.
const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS networks (
    digest     TEXT PRIMARY KEY,
    first_seen INTEGER NOT NULL,
    last_seen  INTEGER NOT NULL
) STRICT;
CREATE TABLE IF NOT EXISTS devices (
    network_digest TEXT NOT NULL,
    device_digest  TEXT NOT NULL,
    first_seen     INTEGER NOT NULL,
    last_seen      INTEGER NOT NULL,
    PRIMARY KEY (network_digest, device_digest)
) STRICT;
";

#[cfg(test)]
mod tests {
    use super::*;
    use jrx_core::history::KeyStrength;

    fn key(digest: &str) -> NetworkKey {
        NetworkKey {
            digest: digest.to_string(),
            strength: KeyStrength::Hardware,
        }
    }

    #[test]
    fn a_network_is_unknown_until_recorded() {
        let s = RecognitionStore::in_memory().unwrap();
        assert!(!s.network_known("aaaa").unwrap());
        s.record_network("aaaa", 100).unwrap();
        assert!(s.network_known("aaaa").unwrap());
    }

    #[test]
    fn first_sight_sets_first_and_last_to_the_same_time() {
        let s = RecognitionStore::in_memory().unwrap();
        s.record_network("aaaa", 100).unwrap();
        assert_eq!(s.network_first_seen("aaaa").unwrap(), Some(100));
    }

    #[test]
    fn a_return_advances_last_seen_but_keeps_first_seen() {
        let s = RecognitionStore::in_memory().unwrap();
        s.record_network("aaaa", 100).unwrap();
        s.record_network("aaaa", 500).unwrap();
        // first_seen is still the first visit.
        assert_eq!(s.network_first_seen("aaaa").unwrap(), Some(100));
        assert_eq!(s.network_count().unwrap(), 1);
    }

    #[test]
    fn observe_reads_first_time_then_returning() {
        let s = RecognitionStore::in_memory().unwrap();
        let k = key("netdigest");

        let (first, first_seen) = s.observe_network(&k, 100).unwrap();
        assert_eq!(first, NetworkRecognition::FirstTime);
        assert_eq!(first_seen, None);

        let (second, seen) = s.observe_network(&k, 900).unwrap();
        assert_eq!(second, NetworkRecognition::Returning);
        assert_eq!(seen, Some(100), "a return reports when it was first seen");
    }

    #[test]
    fn an_addressing_match_is_only_likely_on_return() {
        let s = RecognitionStore::in_memory().unwrap();
        let k = NetworkKey {
            digest: "weak".into(),
            strength: KeyStrength::Addressing,
        };
        assert_eq!(
            s.observe_network(&k, 1).unwrap().0,
            NetworkRecognition::FirstTime
        );
        assert_eq!(
            s.observe_network(&k, 2).unwrap().0,
            NetworkRecognition::ReturningLikely
        );
    }

    #[test]
    fn a_device_is_known_only_on_the_network_it_was_seen_on() {
        let s = RecognitionStore::in_memory().unwrap();
        s.record_device("net-a", "dev-1", 100).unwrap();
        assert!(s.device_known("net-a", "dev-1").unwrap());
        assert!(
            !s.device_known("net-b", "dev-1").unwrap(),
            "the same device on a different network has not been seen there"
        );
    }

    #[test]
    fn observe_device_is_new_then_known() {
        let s = RecognitionStore::in_memory().unwrap();
        assert_eq!(
            s.observe_device("net", Some("dev"), 100).unwrap(),
            DeviceStanding::New
        );
        assert_eq!(
            s.observe_device("net", Some("dev"), 200).unwrap(),
            DeviceStanding::Known
        );
    }

    #[test]
    fn a_device_without_a_key_is_undeterminable_and_unrecorded() {
        let s = RecognitionStore::in_memory().unwrap();
        assert_eq!(
            s.observe_device("net", None, 100).unwrap(),
            DeviceStanding::CannotDetermine
        );
        assert_eq!(
            s.device_count().unwrap(),
            0,
            "an unstable identity is not stored"
        );
    }

    #[test]
    fn sweep_forgets_only_what_is_older_than_the_cutoff() {
        let s = RecognitionStore::in_memory().unwrap();
        s.record_network("old", 100).unwrap();
        s.record_network("fresh", 1_000).unwrap();
        s.record_device("old", "d", 100).unwrap();
        let dropped = s.sweep(500).unwrap();
        assert_eq!(dropped, 2, "the old network and its device go");
        assert!(!s.network_known("old").unwrap());
        assert!(s.network_known("fresh").unwrap());
    }

    #[test]
    fn clear_all_empties_the_store() {
        let s = RecognitionStore::in_memory().unwrap();
        s.record_network("n", 1).unwrap();
        s.record_device("n", "d", 1).unwrap();
        s.clear_all().unwrap();
        assert_eq!(s.network_count().unwrap(), 0);
        assert_eq!(s.device_count().unwrap(), 0);
    }

    #[test]
    fn a_reopened_database_still_remembers() {
        let dir = std::env::temp_dir().join(format!("jrx-store-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("recognition.sqlite3");

        // First run writes and closes the connection.
        {
            let store = RecognitionStore::open(&path).unwrap();
            let (verdict, _) = store.observe_network(&key("persisted"), 42).unwrap();
            assert_eq!(verdict, NetworkRecognition::FirstTime);
        }
        // A fresh run against the same file recognises it.
        {
            let store = RecognitionStore::open(&path).unwrap();
            let (verdict, first_seen) = store.observe_network(&key("persisted"), 99).unwrap();
            assert_eq!(verdict, NetworkRecognition::Returning);
            assert_eq!(
                first_seen,
                Some(42),
                "the original first-seen survives a restart"
            );
        }

        let _ = std::fs::remove_dir_all(&dir);
    }
}
