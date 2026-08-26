//! The session model under load, and the accounting rules that matter most.

use std::time::{Duration, Instant};

use jrx_core::activity::session::ActivitySession;
use jrx_core::activity::{CounterSample, Protocol, SocketObservation};

const TICK: Duration = Duration::from_secs(1);

fn socket(pid: u32, port: u16, bin: u64, bout: u64) -> SocketObservation {
    SocketObservation {
        protocol: Protocol::Tcp,
        local_address: "192.168.1.10".parse().unwrap(),
        local_port: port,
        remote_address: Some(
            format!("104.18.{}.{}", port / 256, port % 256)
                .parse()
                .unwrap(),
        ),
        remote_port: Some(443),
        state: Some("Established".into()),
        rtt_ms: Some(20.0),
        bytes_in: bin,
        bytes_out: bout,
        pid,
        reported_name: format!("program{}", pid % 100),
        executable_path: Some(format!(
            "/Applications/App{}.app/Contents/MacOS/exe",
            pid % 100
        )),
    }
}

/// `count` connections spread over `programs` programs.
fn wave(count: usize, programs: u32, bytes: u64) -> Vec<SocketObservation> {
    (0..count)
        .map(|i| {
            socket(
                (i as u32 % programs) + 1,
                10_000 + i as u16,
                bytes,
                bytes / 2,
            )
        })
        .collect()
}

fn elapsed_ms(f: impl FnOnce()) -> f64 {
    let started = Instant::now();
    f();
    started.elapsed().as_secs_f64() * 1000.0
}

#[test]
fn the_session_keeps_up_from_twenty_to_five_hundred_connections() {
    for (connections, programs) in [(20usize, 5u32), (100, 20), (500, 100)] {
        let mut session = ActivitySession::new("en0");
        session.observe_sockets(wave(connections, programs, 0), TICK);

        let ms = elapsed_ms(|| {
            session.observe_sockets(wave(connections, programs, 5_000), TICK);
        });
        let render = elapsed_ms(|| {
            let _ = session.snapshot(TICK);
        });

        assert!(
            ms < 60.0,
            "{connections} connections took {ms:.1} ms to fold in"
        );
        assert!(
            render < 60.0,
            "{connections} connections took {render:.1} ms to render"
        );

        let snapshot = session.snapshot(TICK);
        assert_eq!(snapshot.programs.len(), programs as usize);
        assert_eq!(snapshot.active_connections, connections);
    }
}

/// Every byte observed has to land somewhere, whatever the scale.
#[test]
fn nothing_is_lost_or_double_counted_at_five_hundred_connections() {
    let mut session = ActivitySession::new("en0");
    session.observe_sockets(wave(500, 100, 0), TICK);
    session.observe_sockets(wave(500, 100, 4_000), TICK);

    let snapshot = session.snapshot(TICK);
    let total: u64 = snapshot.programs.iter().map(|p| p.session_bytes_in).sum();
    assert_eq!(
        total,
        500 * 4_000,
        "every socket's delta must be counted once"
    );
}

/// A long session must not grow without bound. Programs that fall silent are
/// eventually dropped; the ones still talking are kept.
#[test]
fn a_long_session_does_not_grow_without_bound() {
    let mut session = ActivitySession::new("en0");

    // A thousand programs come and go.
    for round in 0..10u32 {
        let wave: Vec<SocketObservation> = (0..100)
            .map(|i| socket(round * 1000 + i, 20_000 + i as u16, 0, 0))
            .collect();
        session.observe_sockets(wave, TICK);
    }
    for _ in 0..ActivitySession::FORGET_AFTER_IDLE_SAMPLES + 2 {
        session.observe_sockets(vec![], TICK);
    }

    assert_eq!(
        session.programs().count(),
        0,
        "programs silent for the whole timeout must be forgotten"
    );
}

/// The scenario from the brief, end to end.
#[test]
fn traffic_observed_before_a_socket_closed_is_still_reported_afterwards() {
    let mut session = ActivitySession::new("en0");

    // t0: Safari's socket carries 10 MB, none of it watched by JRX.
    session.observe_sockets(vec![socket(500, 52000, 10_000_000, 0)], TICK);
    assert_eq!(session.programs().next().unwrap().session_bytes_in, 0);

    // t1: it reaches 12 MB. JRX watched 2 MB of that.
    session.observe_sockets(vec![socket(500, 52000, 12_000_000, 0)], TICK);
    assert_eq!(
        session.programs().next().unwrap().session_bytes_in,
        2_000_000
    );

    // t2: the socket closes and disappears from the observation entirely.
    session.observe_sockets(vec![], TICK);

    let safari = session.programs().next().expect("Safari is still listed");
    assert_eq!(safari.session_bytes_in, 2_000_000, "the 2 MB must survive");
    assert_eq!(safari.active_connections, 0);
    assert_eq!(safari.rate_in, 0);
}

// ---- interface counters under real conditions ----

#[test]
fn sleep_and_wake_does_not_invent_traffic() {
    let mut session = ActivitySession::new("en0");
    session.observe_counters(
        CounterSample {
            rx_bytes: 5_000_000,
            tx_bytes: 1_000_000,
        },
        TICK,
    );
    session.observe_counters(
        CounterSample {
            rx_bytes: 5_100_000,
            tx_bytes: 1_010_000,
        },
        TICK,
    );

    // Woke up; the interface reinitialised and its counters restarted.
    session.observe_counters(
        CounterSample {
            rx_bytes: 2_000,
            tx_bytes: 500,
        },
        TICK,
    );
    assert_eq!(
        session.session_bytes_in(),
        100_000,
        "the reset adds nothing"
    );

    // Counting resumes from the new baseline.
    session.observe_counters(
        CounterSample {
            rx_bytes: 12_000,
            tx_bytes: 1_500,
        },
        TICK,
    );
    assert_eq!(session.session_bytes_in(), 110_000);
}

#[test]
fn reconnecting_on_a_different_interface_keeps_what_was_already_observed() {
    let mut session = ActivitySession::new("en0");
    session.observe_counters(
        CounterSample {
            rx_bytes: 1_000,
            tx_bytes: 0,
        },
        TICK,
    );
    session.observe_counters(
        CounterSample {
            rx_bytes: 6_000,
            tx_bytes: 0,
        },
        TICK,
    );

    session.switch_interface("en7");
    session.observe_counters(
        CounterSample {
            rx_bytes: 900_000_000,
            tx_bytes: 0,
        },
        TICK,
    );

    assert_eq!(
        session.session_bytes_in(),
        5_000,
        "the new interface adds nothing yet"
    );
    session.observe_counters(
        CounterSample {
            rx_bytes: 900_002_000,
            tx_bytes: 0,
        },
        TICK,
    );
    assert_eq!(session.session_bytes_in(), 7_000);
}
