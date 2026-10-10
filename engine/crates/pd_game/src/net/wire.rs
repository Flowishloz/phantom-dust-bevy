//! Byte encoding of `NetMsg` for the UDP transport (one message per datagram).
//!
//! Every packet: `b"PDN"`, the `PROTOCOL` byte, a tag byte, then the fields,
//! little-endian. Strings are a `u8` length + UTF-8 (at most `MAX_STR`
//! bytes), vectors a `u8` count + items. Decoding never panics: garbage,
//! truncated, oversized or trailing-byte input gives `None`.
//!
//! Besides the `NetMsg` tags the UDP transport adds two framing tags of its
//! own (`Frame::Reliable`, `Frame::Ack`) for the lobby messages that must
//! arrive exactly once and in order (see `udp.rs`).

use super::{LobbyPick, LobbyRules, NetMsg, PROTOCOL, RESEND};
use crate::sim::Input;

pub const MAGIC: [u8; 3] = *b"PDN";
/// Longest string on the wire (bytes); longer ones are cut at a char boundary.
pub const MAX_STR: usize = 64;
/// Most arsenal entries in a `LobbyPick`.
pub const MAX_ARSENAL: usize = 64;
/// Most inputs in one `Inputs` message.
pub const MAX_INPUTS: usize = RESEND * 2;
/// Largest datagram we ever produce (well under the ~1200-byte safe MTU).
pub const MAX_PACKET: usize = 1200;

const T_HELLO: u8 = 1;
const T_PICK: u8 = 2;
const T_RULES: u8 = 3;
const T_START: u8 = 4;
const T_INPUTS: u8 = 5;
const T_CHECKSUM: u8 = 6;
const T_PING: u8 = 7;
const T_PONG: u8 = 8;
const T_BYE: u8 = 9;
const T_RELIABLE: u8 = 0x20;
const T_ACK: u8 = 0x21;

/// One datagram as the UDP transport sees it.
#[derive(Clone, Debug, PartialEq)]
pub enum Frame {
    /// Sent once, may be lost (Hello, Inputs, Ping, Pong, Bye).
    Plain(NetMsg),
    /// Resent until acked, delivered once and in `seq` order (Pick, Rules,
    /// Start, Checksum).
    Reliable { seq: u32, msg: NetMsg },
    /// "I have reliable message `seq`".
    Ack(u32),
}

/// A plain `NetMsg` packet.
pub fn encode(msg: &NetMsg) -> Vec<u8> {
    encode_frame(&Frame::Plain(msg.clone()))
}

pub fn decode(bytes: &[u8]) -> Option<NetMsg> {
    match decode_frame(bytes)? {
        Frame::Plain(m) => Some(m),
        _ => None,
    }
}

pub fn encode_frame(frame: &Frame) -> Vec<u8> {
    let mut w = Vec::with_capacity(64);
    w.extend_from_slice(&MAGIC);
    w.push(PROTOCOL as u8);
    match frame {
        Frame::Plain(m) => put_msg(&mut w, m),
        Frame::Reliable { seq, msg } => {
            w.push(T_RELIABLE);
            w.extend_from_slice(&seq.to_le_bytes());
            put_msg(&mut w, msg);
        }
        Frame::Ack(seq) => {
            w.push(T_ACK);
            w.extend_from_slice(&seq.to_le_bytes());
        }
    }
    debug_assert!(w.len() < MAX_PACKET, "net packet too big: {} bytes", w.len());
    w
}

pub fn decode_frame(bytes: &[u8]) -> Option<Frame> {
    if bytes.len() > MAX_PACKET {
        return None;
    }
    let mut r = Reader { b: bytes };
    if r.take(3)? != MAGIC || r.u8()? != PROTOCOL as u8 {
        return None;
    }
    let frame = match r.peek()? {
        T_RELIABLE => {
            r.u8()?;
            let seq = r.u32()?;
            Frame::Reliable { seq, msg: get_msg(&mut r)? }
        }
        T_ACK => {
            r.u8()?;
            Frame::Ack(r.u32()?)
        }
        _ => Frame::Plain(get_msg(&mut r)?),
    };
    r.b.is_empty().then_some(frame)
}

fn put_msg(w: &mut Vec<u8>, m: &NetMsg) {
    match m {
        NetMsg::Hello { build, data_hash } => {
            w.push(T_HELLO);
            put_str(w, build);
            w.extend_from_slice(&data_hash.to_le_bytes());
        }
        NetMsg::Pick(p) => {
            w.push(T_PICK);
            put_pick(w, p);
        }
        NetMsg::Rules(r) => {
            w.push(T_RULES);
            put_rules(w, r);
        }
        NetMsg::Start { rules, picks } => {
            w.push(T_START);
            put_rules(w, rules);
            put_pick(w, &picks[0]);
            put_pick(w, &picks[1]);
        }
        NetMsg::Inputs { first_tick, inputs } => {
            w.push(T_INPUTS);
            // Keep the newest if a caller ever passes more than fit.
            let skip = inputs.len().saturating_sub(MAX_INPUTS);
            w.extend_from_slice(&(first_tick + skip as u64).to_le_bytes());
            w.push((inputs.len() - skip) as u8);
            for i in &inputs[skip..] {
                w.extend_from_slice(i);
            }
        }
        NetMsg::Checksum { tick, hash } => {
            w.push(T_CHECKSUM);
            w.extend_from_slice(&tick.to_le_bytes());
            w.extend_from_slice(&hash.to_le_bytes());
        }
        NetMsg::Ping(t) => {
            w.push(T_PING);
            w.extend_from_slice(&t.to_le_bytes());
        }
        NetMsg::Pong(t) => {
            w.push(T_PONG);
            w.extend_from_slice(&t.to_le_bytes());
        }
        NetMsg::Bye => w.push(T_BYE),
    }
}

fn get_msg(r: &mut Reader) -> Option<NetMsg> {
    Some(match r.u8()? {
        T_HELLO => NetMsg::Hello { build: r.str()?, data_hash: r.u64()? },
        T_PICK => NetMsg::Pick(get_pick(r)?),
        T_RULES => NetMsg::Rules(get_rules(r)?),
        T_START => NetMsg::Start { rules: get_rules(r)?, picks: [get_pick(r)?, get_pick(r)?] },
        T_INPUTS => {
            let first_tick = r.u64()?;
            let n = r.u8()? as usize;
            if n > MAX_INPUTS {
                return None;
            }
            let mut inputs = Vec::with_capacity(n);
            for _ in 0..n {
                inputs.push(<[u8; Input::BYTES]>::try_from(r.take(Input::BYTES)?).ok()?);
            }
            first_tick.checked_add(n as u64)?;
            NetMsg::Inputs { first_tick, inputs }
        }
        T_CHECKSUM => NetMsg::Checksum { tick: r.u64()?, hash: r.u64()? },
        T_PING => NetMsg::Ping(r.u32()?),
        T_PONG => NetMsg::Pong(r.u32()?),
        T_BYE => NetMsg::Bye,
        _ => return None,
    })
}

fn put_str(w: &mut Vec<u8>, s: &str) {
    let mut end = s.len().min(MAX_STR);
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    w.push(end as u8);
    w.extend_from_slice(&s.as_bytes()[..end]);
}

fn put_pick(w: &mut Vec<u8>, p: &LobbyPick) {
    put_str(w, &p.char);
    w.push(p.colour);
    let n = p.arsenal.len().min(MAX_ARSENAL);
    w.push(n as u8);
    for s in &p.arsenal[..n] {
        w.extend_from_slice(&s.to_le_bytes());
    }
    w.push(p.ready as u8);
}

fn get_pick(r: &mut Reader) -> Option<LobbyPick> {
    let char = r.str()?;
    let colour = r.u8()?;
    let n = r.u8()? as usize;
    if n > MAX_ARSENAL {
        return None;
    }
    let mut arsenal = Vec::with_capacity(n);
    for _ in 0..n {
        arsenal.push(i16::from_le_bytes(r.take(2)?.try_into().ok()?));
    }
    let ready = r.bool()?;
    Some(LobbyPick { char, colour, arsenal, ready })
}

fn put_rules(w: &mut Vec<u8>, r: &LobbyRules) {
    put_str(w, &r.stage);
    w.extend_from_slice(&r.time_limit_ticks.to_le_bytes());
    match r.rounds_to_win {
        None => w.push(0),
        Some(n) => {
            w.push(1);
            w.extend_from_slice(&n.to_le_bytes());
        }
    }
    put_str(w, &r.mode);
    w.extend_from_slice(&r.rules_hash.to_le_bytes());
}

fn get_rules(r: &mut Reader) -> Option<LobbyRules> {
    let stage = r.str()?;
    let time_limit_ticks = r.u32()?;
    let rounds_to_win = if r.bool()? { Some(r.u32()?) } else { None };
    let mode = r.str()?;
    let rules_hash = r.u64()?;
    Some(LobbyRules { stage, time_limit_ticks, rounds_to_win, mode, rules_hash })
}

struct Reader<'a> {
    b: &'a [u8],
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        if self.b.len() < n {
            return None;
        }
        let (head, rest) = self.b.split_at(n);
        self.b = rest;
        Some(head)
    }
    fn peek(&self) -> Option<u8> {
        self.b.first().copied()
    }
    fn u8(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }
    fn bool(&mut self) -> Option<bool> {
        match self.u8()? {
            0 => Some(false),
            1 => Some(true),
            _ => None,
        }
    }
    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }
    fn u64(&mut self) -> Option<u64> {
        Some(u64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }
    fn str(&mut self) -> Option<String> {
        let n = self.u8()? as usize;
        if n > MAX_STR {
            return None;
        }
        String::from_utf8(self.take(n)?.to_vec()).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pick(seed: u8) -> LobbyPick {
        LobbyPick { char: format!("pc0{seed}a"), colour: seed, arsenal: (0..30).map(|i| i * 37 - 500 + seed as i16).collect(), ready: seed % 2 == 1 }
    }

    fn rules() -> LobbyRules {
        LobbyRules { stage: "st03".into(), time_limit_ticks: 60 * 180, rounds_to_win: Some(2), mode: "close_quarters".into(), rules_hash: 0x1234_5678_9abc_def0 }
    }

    fn all_msgs() -> Vec<NetMsg> {
        vec![
            NetMsg::Hello { build: super::super::build_id(), data_hash: 0xDEAD_BEEF_0123_4567 },
            NetMsg::Hello { build: String::new(), data_hash: 0 },
            NetMsg::Pick(pick(1)),
            NetMsg::Pick(LobbyPick::default()),
            NetMsg::Pick(LobbyPick { char: "x".repeat(MAX_STR), arsenal: vec![i16::MIN; MAX_ARSENAL], ..pick(2) }),
            NetMsg::Rules(rules()),
            NetMsg::Rules(LobbyRules { stage: "st00".into(), time_limit_ticks: 0, rounds_to_win: None, mode: String::new(), rules_hash: 0 }),
            NetMsg::Start { rules: rules(), picks: [pick(3), pick(4)] },
            NetMsg::Inputs { first_tick: 1234, inputs: (0..RESEND as u8).map(|k| [k, 255 - k, k * 3, k & 15, 128]).collect() },
            NetMsg::Inputs { first_tick: 0, inputs: vec![] },
            NetMsg::Inputs { first_tick: u64::MAX - 20, inputs: vec![[1; Input::BYTES]; MAX_INPUTS] },
            NetMsg::Checksum { tick: 600, hash: u64::MAX },
            NetMsg::Ping(0),
            NetMsg::Pong(u32::MAX),
            NetMsg::Bye,
        ]
    }

    #[test]
    fn every_variant_round_trips() {
        for m in all_msgs() {
            let b = encode(&m);
            assert!(b.len() < MAX_PACKET);
            assert_eq!(&b[..3], b"PDN");
            assert_eq!(decode(&b), Some(m.clone()), "{m:?}");
            let r = Frame::Reliable { seq: 77, msg: m.clone() };
            assert_eq!(decode_frame(&encode_frame(&r)), Some(r));
        }
        assert_eq!(decode_frame(&encode_frame(&Frame::Ack(9))), Some(Frame::Ack(9)));
        assert_eq!(decode(&encode_frame(&Frame::Ack(9))), None, "framing tags are not NetMsgs");
    }

    #[test]
    fn biggest_message_fits_one_datagram() {
        let big = LobbyPick { char: "é".repeat(100), colour: 255, arsenal: vec![-1; 200], ready: true };
        let m = NetMsg::Start { rules: LobbyRules { stage: "s".repeat(500), time_limit_ticks: u32::MAX, rounds_to_win: Some(u32::MAX), mode: "m".repeat(500), rules_hash: u64::MAX }, picks: [big.clone(), big] };
        let b = encode_frame(&Frame::Reliable { seq: u32::MAX, msg: m });
        assert!(b.len() < MAX_PACKET, "{}", b.len());
        // Over-long fields are cut (at a char boundary), not rejected.
        let Some(Frame::Reliable { msg: NetMsg::Start { picks, rules }, .. }) = decode_frame(&b) else { panic!() };
        assert_eq!(picks[0].char, "é".repeat(MAX_STR / 2));
        assert_eq!(picks[0].arsenal.len(), MAX_ARSENAL);
        assert_eq!(rules.stage.len(), MAX_STR);
        // Too many inputs: the newest are kept, with the right first tick.
        let m = NetMsg::Inputs { first_tick: 10, inputs: (0..40u8).map(|k| [k; Input::BYTES]).collect() };
        let Some(NetMsg::Inputs { first_tick, inputs }) = decode(&encode(&m)) else { panic!() };
        assert_eq!((first_tick, inputs.len(), inputs[0][0]), (34, MAX_INPUTS, 24));
    }

    #[test]
    fn truncations_and_extensions_are_rejected() {
        for m in all_msgs() {
            let b = encode(&m);
            for n in 0..b.len() {
                assert_eq!(decode_frame(&b[..n]), None, "{m:?} cut to {n}");
            }
            let mut long = b.clone();
            long.push(0);
            assert_eq!(decode_frame(&long), None, "trailing byte");
        }
        let mut b = encode(&NetMsg::Bye);
        b[3] = PROTOCOL as u8 + 1;
        assert_eq!(decode(&b), None, "other protocol");
        assert_eq!(decode(&[0u8; 2000]), None);
    }

    #[test]
    fn random_bytes_never_panic() {
        // xorshift: deterministic "fuzz".
        let mut s = 0x9E37_79B9_7F4A_7C15u64;
        let mut next = move || {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            s
        };
        let seeds: Vec<Vec<u8>> = all_msgs().iter().map(encode).collect();
        for k in 0..20_000 {
            let mut b: Vec<u8> = if k % 2 == 0 {
                let len = (next() % 300) as usize;
                (0..len).map(|_| next() as u8).collect()
            } else {
                seeds[k / 2 % seeds.len()].clone()
            };
            if k % 2 == 0 && k % 4 == 0 && b.len() >= 5 {
                b[..4].copy_from_slice(&[b'P', b'D', b'N', PROTOCOL as u8]);
            }
            // Flip a few bytes of a valid packet.
            for _ in 0..(next() % 4) {
                if !b.is_empty() {
                    let i = (next() as usize) % b.len();
                    b[i] = next() as u8;
                }
            }
            if let Some(f) = decode_frame(&b) {
                // Whatever decodes re-encodes to the same bytes.
                assert_eq!(encode_frame(&f), b);
            }
        }
    }
}
