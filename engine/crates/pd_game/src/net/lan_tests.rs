//! `NetSession` over real UDP sockets on 127.0.0.1 (ephemeral ports, one
//! test process), plus `data_hash` / `open_lan_*`.

use super::udp::Udp;
use super::*;
use std::time::{Duration, Instant};

/// Host + joiner sessions over UDP; `loss` = lose about 1 in `loss`
/// datagrams on both ends (0 = none).
fn pair(host_hash: u64, join_hash: u64, loss: usize) -> (NetSession, NetSession) {
    let mut h = Udp::host(0).unwrap();
    let mut c = Udp::join(&format!("127.0.0.1:{}", h.local_port())).unwrap();
    h.set_test_loss(loss);
    c.set_test_loss(loss);
    (NetSession::new(Role::Host, Box::new(h), "test".into(), host_hash), NetSession::new(Role::Client, Box::new(c), "test".into(), join_hash))
}

/// Poll both until `done` (5 s cap); returns every event each side saw.
fn pump(h: &mut NetSession, c: &mut NetSession, mut done: impl FnMut(&NetSession, &NetSession, &[LobbyEvent], &[LobbyEvent]) -> bool) -> (Vec<LobbyEvent>, Vec<LobbyEvent>) {
    let (mut he, mut ce) = (Vec::new(), Vec::new());
    let deadline = Instant::now() + Duration::from_secs(5);
    while !done(h, c, &he, &ce) {
        assert!(Instant::now() < deadline, "timed out: host {:?} {he:?} / client {:?} {ce:?}", h.status, c.status);
        he.extend(h.poll());
        ce.extend(c.poll());
        std::thread::sleep(Duration::from_millis(1));
    }
    (he, ce)
}

fn connected(loss: usize) -> (NetSession, NetSession) {
    let (mut h, mut c) = pair(5, 5, loss);
    let (he, ce) = pump(&mut h, &mut c, |h, c, _, _| h.status == Status::Connected && c.status == Status::Connected);
    assert_eq!(he, vec![LobbyEvent::Connected]);
    assert_eq!(ce, vec![LobbyEvent::Connected]);
    (h, c)
}

fn pick(seed: u8) -> LobbyPick {
    LobbyPick { char: format!("pc0{seed}a"), colour: seed, arsenal: (0..30).map(|k| k * 11 + seed as i16).collect(), ready: true }
}

fn lobby(h: &mut NetSession, c: &mut NetSession) {
    let rules = LobbyRules { stage: "st03".into(), time_limit_ticks: 10_800, rounds_to_win: None, mode: "close_quarters".into(), rules_hash: 77 };
    c.send(NetMsg::Pick(pick(2)));
    h.send(NetMsg::Pick(pick(1)));
    h.send(NetMsg::Rules(rules.clone()));
    h.send(NetMsg::Start { rules: rules.clone(), picks: [pick(1), pick(2)] });
    let (he, ce) = pump(h, c, |_, _, he, ce| !he.is_empty() && ce.len() >= 3);
    assert_eq!(he, vec![LobbyEvent::PeerPick(pick(2))]);
    assert_eq!(ce, vec![LobbyEvent::PeerPick(pick(1)), LobbyEvent::Rules(rules.clone()), LobbyEvent::Start { rules, picks: [pick(1), pick(2)] }]);
}

fn input_of(slot: usize, t: u64) -> Input {
    Input { stick: [(t as i64 % 255 - 127) as i8, slot as i8 * 50 - 25], jump: t % 7 == slot as u64, lock: t % 3 == 0, skills: (t % 8) as u8, cam: (t % 5) as i8 - 2, cam_fast: t % 13 == 0, recenter: false, switch_target: t % 11 == 0 }
}

/// Run `ticks` lockstep ticks on both sides the way the match glue will:
/// push this tick's input once, step only when `inputs_for` has both.
fn lockstep(h: &mut NetSession, c: &mut NetSession, ticks: u64) -> [Vec<[Input; 2]>; 2] {
    let mut logs = [Vec::new(), Vec::new()];
    let mut tick = [0u64; 2];
    let mut pushed = [None::<u64>; 2];
    let deadline = Instant::now() + Duration::from_secs(60);
    while tick[0] < ticks || tick[1] < ticks {
        assert!(Instant::now() < deadline, "lockstep stalled at {tick:?}");
        for (k, s) in [&mut *h, &mut *c].into_iter().enumerate() {
            s.poll();
            assert_eq!(s.status, Status::Connected);
            while tick[k] < ticks {
                let t = tick[k];
                if pushed[k] != Some(t) {
                    s.push_local_input(t, input_of(s.my_slot(), t));
                    pushed[k] = Some(t);
                }
                let Some(both) = s.inputs_for(t) else { break };
                logs[k].push(both);
                s.forget_before(t);
                tick[k] += 1;
                if tick[k] % CHECKSUM_EVERY == 0 {
                    // Stand-in for Match::checksum(): a hash of the inputs so far.
                    let sum = logs[k].iter().flatten().fold(0u64, |a, i| a.wrapping_mul(31) ^ u64::from_le_bytes([i.to_bytes(), [0; 5]].concat()[..8].try_into().unwrap()));
                    s.report_checksum(tick[k], sum);
                }
            }
        }
        std::thread::sleep(Duration::from_micros(200));
    }
    // Let the last checksums cross.
    pump(h, c, |h, c, _, _| h.remote_sums.is_empty() && c.remote_sums.is_empty() && h.local_sums.is_empty() && c.local_sums.is_empty());
    logs
}

fn check_logs(logs: &[Vec<[Input; 2]>; 2], ticks: u64) {
    assert_eq!(logs[0].len() as u64, ticks);
    assert_eq!(logs[0], logs[1], "both machines saw the same inputs every tick");
    for (t, both) in logs[0].iter().enumerate() {
        for slot in 0..2 {
            let want = if (t as u64) < INPUT_DELAY { Input::default() } else { input_of(slot, t as u64 - INPUT_DELAY) };
            assert_eq!(both[slot], want, "tick {t} slot {slot}");
        }
    }
}

#[test]
fn lan_host_and_join_connect_and_lobby_arrives() {
    let (mut h, mut c) = connected(0);
    lobby(&mut h, &mut c);
    pump(&mut h, &mut c, |h, c, _, _| h.ping_ms().is_some() && c.ping_ms().is_some());
    assert!(h.ping_ms().unwrap() < 1000);
}

#[test]
fn lan_lockstep_600_ticks_identical_on_both() {
    let (mut h, mut c) = connected(0);
    lobby(&mut h, &mut c);
    let logs = lockstep(&mut h, &mut c, 600);
    check_logs(&logs, 600);
    assert_eq!((h.desync_tick, c.desync_tick), (None, None));
}

#[test]
fn lan_lockstep_survives_packet_loss() {
    // About one datagram in three lost, both ways, from the very first Hello.
    let (mut h, mut c) = connected(3);
    lobby(&mut h, &mut c);
    let logs = lockstep(&mut h, &mut c, 600);
    check_logs(&logs, 600);
    assert_eq!((h.desync_tick, c.desync_tick), (None, None));
    // A rematch starts clean at tick 0 even with old packets in flight.
    lobby(&mut h, &mut c);
    let logs = lockstep(&mut h, &mut c, 120);
    check_logs(&logs, 120);
}

#[test]
fn lan_data_mismatch_closes_both_with_reason() {
    let (mut h, mut c) = pair(1, 2, 0);
    let (he, ce) = pump(&mut h, &mut c, |h, c, _, _| matches!(h.status, Status::Closed(_)) && matches!(c.status, Status::Closed(_)));
    for ev in [he, ce] {
        let [LobbyEvent::Closed(reason)] = &ev[..] else { panic!("{ev:?}") };
        assert!(reason.contains("Different converted game data"), "{reason}");
    }
}

#[test]
fn lan_bye_closes_the_peer() {
    let (mut h, mut c) = connected(0);
    c.leave();
    let (he, _) = pump(&mut h, &mut c, |h, _, _, _| h.status != Status::Connected);
    assert_eq!(he, vec![LobbyEvent::Closed("The other player left".into())]);
}

#[test]
fn lan_silence_closes_and_unreachable_host_gives_up() {
    let (mut h, mut c) = connected(0);
    // Stop polling the client; drain what it already sent.
    std::thread::sleep(Duration::from_millis(30));
    h.poll();
    assert_eq!(h.poll_at(Instant::now() + Duration::from_secs(4)), vec![]);
    assert_eq!(h.poll_at(Instant::now() + Duration::from_secs(6)), vec![LobbyEvent::Closed("Connection lost".into())]);
    drop(c.poll());

    // Nobody listening on the dialled port (Windows answers with
    // ConnectionReset, which must not break anything).
    let gone = std::net::UdpSocket::bind("127.0.0.1:0").unwrap().local_addr().unwrap();
    let mut j = NetSession::new(Role::Client, Box::new(Udp::join(&gone.to_string()).unwrap()), "test".into(), 0);
    j.poll();
    std::thread::sleep(Duration::from_millis(20));
    assert_eq!(j.poll_at(Instant::now() + Duration::from_secs(9)), vec![]);
    assert_eq!(j.poll_at(Instant::now() + Duration::from_secs(11)), vec![LobbyEvent::Closed("Could not reach host".into())]);

    // A host with nobody joining waits forever.
    let mut lonely = NetSession::new(Role::Host, Box::new(Udp::host(0).unwrap()), "test".into(), 0);
    lonely.poll();
    assert_eq!(lonely.poll_at(Instant::now() + Duration::from_secs(600)), vec![]);
    assert_eq!(lonely.status, Status::Waiting);
}

fn temp_cache(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("pd_net_test_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("chars")).unwrap();
    d
}

#[test]
fn data_hash_covers_sim_data_only() {
    let d = temp_cache("hash");
    let empty = data_hash(&d);
    assert_eq!(empty, data_hash(&d), "stable");
    assert_eq!(data_hash(&d.join("does-not-exist")), empty, "missing files hash as absent, no error");
    std::fs::write(d.join("skills.json"), "[1]").unwrap();
    let a = data_hash(&d);
    assert_ne!(a, empty);
    // Ignored: models, textures, sounds, other json.
    std::fs::write(d.join("st00.glb"), "x").unwrap();
    std::fs::write(d.join("st00.bounds.json"), "x").unwrap();
    std::fs::write(d.join("chars").join("pc00a.glb"), "x").unwrap();
    assert_eq!(data_hash(&d), a);
    // Counted: collision, props, root motion, and their contents.
    let mut seen = vec![a];
    for (f, body) in [("st00.pdcl", "c"), ("st00.props.json", "p"), ("chars/pc00a.rootmotion.json", "r"), ("st00.pdcl", "c2"), ("chars.json", "{}"), ("stages.json", "{}")] {
        std::fs::write(d.join(f), body).unwrap();
        let h = data_hash(&d);
        assert!(!seen.contains(&h), "{f} changed the hash");
        seen.push(h);
    }
    // Same content in a second folder gives the same hash.
    let e = temp_cache("hash2");
    for f in ["skills.json", "chars.json", "stages.json", "st00.pdcl", "st00.props.json", "chars/pc00a.rootmotion.json"] {
        std::fs::copy(d.join(f), e.join(f)).unwrap();
    }
    assert_eq!(data_hash(&e), *seen.last().unwrap());
    let _ = std::fs::remove_dir_all(&d);
    let _ = std::fs::remove_dir_all(&e);
}

#[test]
fn open_lan_host_and_join_talk() {
    let d = temp_cache("open");
    let mut h = open_lan_host(0, &d).unwrap();
    let port = h.local_port().unwrap();
    assert!(port != 0);
    let mut c = open_lan_join(&format!("127.0.0.1:{port}"), &d).unwrap();
    pump(&mut h, &mut c, |h, c, _, _| h.status == Status::Connected && c.status == Status::Connected);
    assert!(open_lan_join("", &d).is_err());
    // The port is taken now.
    assert!(open_lan_host(port, &d).is_err());
    let _ = std::fs::remove_dir_all(&d);
}

/// `PD_CACHE=<engine/cache> cargo test -p pd_game net::lan_tests::data_hash_real_cache -- --ignored --nocapture`
#[test]
#[ignore]
fn data_hash_real_cache() {
    let cache = std::path::PathBuf::from(std::env::var("PD_CACHE").expect("set PD_CACHE"));
    let t = Instant::now();
    let h = data_hash(&cache);
    println!("data_hash = {h:016x} in {:?}", t.elapsed());
    assert!(t.elapsed() < Duration::from_secs(1));
}
