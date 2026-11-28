//! The paired form of a console wave64 compute program on a host of 32-lane waves: each host
//! lane carries the values of two console lanes, and one host wave runs a whole console wave
//! (host lane 7 runs console lanes 7 and 39). After the approach of a patch published for a
//! program built on this same code (whose header names "XCL 3ecf4cf"), written here in this
//! tree's terms.
//!
//! The program comes lowered as for a host wave of 64 lanes, the console's own
//! (`lower::with_paired`), with every branch on a lane mask taken wave-wide
//! ([`crate::structure::structure_wide`]): its control flow is the console wave's, the same for
//! all 64 lanes, and EXEC is data (each vector instruction's guard). What branches per lane is
//! the code of single instructions (a store under its guard, say). Every instruction of that
//! module is written twice here, once per half ("bank"): bank 0 holds console lanes 0..31,
//! bank 1 lanes 32..63, in the same host lanes. The wave's decisions are taken once. A wave
//! operation sees both banks: a ballot's low word is bank 0's, its high word bank 1's; a
//! reduction combines the two banks' results; a lane read takes the lane of the half its
//! index names. An instruction whose code branches per lane runs it for bank 0, then for
//! bank 1, before the next one (a "region"). An instruction that has the same operands in
//! both banks and no effect (scalar code, handles, loads of one address) is written once.

use super::*;
use crate::emit::shader::{Shader, flags};

type Pair = [V; 2];
type Result<T> = std::result::Result<T, &'static str>;

/// The DXIL blocks of one instruction whose code branches per lane: it starts at the end of
/// `head` and joins again in `join`. Bank 0 goes through its own copy of the blocks between to
/// `middle`, bank 1 through the original ones from `middle` to `join`.
#[derive(Clone, Copy)]
struct Region {
    head: usize,
    join: usize,
    middle: usize,
}

/// Where each block of the lowered module goes: one block for both banks, or one per bank
/// inside a region.
struct Blocks {
    ids: Vec<[usize; 2]>,
    starts: Vec<Option<Region>>,
    ends: Vec<Option<Region>>,
    inside: Vec<Option<Region>>,
}

impl Blocks {
    fn new(m: &mut Module, source: &[Block], regions: &[(usize, usize)]) -> Result<Self> {
        let n = source.len();
        let mut dest = Self {
            ids: (0..n).map(|_| [m.new_block(); 2]).collect(),
            starts: vec![None; n],
            ends: vec![None; n],
            inside: vec![None; n],
        };
        for &(head, join) in regions {
            let region = Region {
                head,
                join,
                middle: m.new_block(),
            };
            if dest.starts[head].replace(region).is_some()
                || dest.ends[join].replace(region).is_some()
            {
                return Err("paired: two instructions branching from one block");
            }
            let mut pending = successors(source[head].term).to_vec();
            while let Some(b) = pending.pop() {
                if b == join || dest.inside[b].is_some_and(|r| r.head == head) {
                    continue;
                }
                if b == head
                    || dest.inside[b].replace(region).is_some()
                    || source[b].term.is_none_or(|t| matches!(t, Term::Ret))
                {
                    return Err("paired: an instruction whose code does not join again");
                }
                dest.ids[b][1] = m.new_block();
                pending.extend_from_slice(&successors(source[b].term));
            }
        }
        Ok(dest)
    }

    /// The block that bank `bank`'s copy of the terminator of `b` closes.
    fn from(&self, b: usize, bank: usize) -> usize {
        if bank == 1
            && let Some(r) = self.starts[b]
        {
            return r.middle;
        }
        self.ids[b][bank]
    }

    /// Where bank `bank`'s copy of the edge from `from` to `to` goes.
    fn to(&self, from: usize, to: usize, bank: usize) -> usize {
        if bank == 0
            && let Some(r) = self.starts[from].or(self.inside[from])
            && to == r.join
        {
            return r.middle;
        }
        self.ids[to][bank]
    }

    /// Whether the code of `b` is the same block for both banks.
    fn shared(&self, b: usize) -> bool {
        self.ids[b][0] == self.ids[b][1]
    }
}

/// The two banks' values of each instruction of the lowered module.
struct Values(Vec<Option<Pair>>);

impl Values {
    fn get(&self, v: V) -> Result<Pair> {
        match v {
            V::Inst(i) => self.0[i as usize].ok_or("paired: a value read before it is written"),
            _ => Ok([v; 2]),
        }
    }

    /// `op` with bank `bank`'s values.
    fn operation(&self, op: &Op, bank: usize) -> Result<Op> {
        let mut copy = op.clone();
        let mut valid = Ok(());
        opt::map_operands(&mut copy, |v| match self.get(v) {
            Ok(pair) => pair[bank],
            Err(e) => {
                valid = Err(e);
                v
            }
        });
        valid.map(|()| copy)
    }

    /// Whether every value `op` reads is the same in both banks.
    fn alike(&self, op: &Op) -> bool {
        let mut same = true;
        for_each_operand(op, |v| same &= self.get(v).is_ok_and(|p| p[0] == p[1]));
        same
    }
}

/// The blocks in reverse postorder: definitions before their uses, but for joins over back
/// edges, which are wired at the end.
fn order(blocks: &[Block]) -> Vec<usize> {
    let mut out = Vec::new();
    let mut seen = vec![false; blocks.len()];
    let mut pending = vec![(0, false)];
    while let Some((b, leaving)) = pending.pop() {
        if leaving {
            out.push(b);
        } else if !std::mem::replace(&mut seen[b], true) {
            pending.push((b, true));
            pending.extend(successors(blocks[b].term).iter().map(|&s| (s, false)));
        }
    }
    out.reverse();
    out
}

/// The console lane of each bank (0..63) and the console thread of each in the group. A group
/// of one console wave is one host wave: its threads are the lanes. A group of several numbers
/// its host waves from an LDS counter (`counter`): a GPU need not give a host wave consecutive
/// thread indices, so every thread zeroes it, the group meets, the first lane of each host wave
/// adds 64 and the wave takes what it found. In the new entry block, which every thread runs.
/// `counter`: the dword, and whether the program uses it too (its LDS fills what the host has):
/// then the group meets once more before any wave goes on to write it.
fn thread_numbers(s: &mut Shader, counter: Option<(u32, bool)>) -> (Pair, Pair) {
    let b = s.entry;
    let i32t = s.t.i32;
    let lane = s.op(b, "waveGetLaneIndex", Attr::ReadOnly, i32t, 111, &[]);
    let k32 = s.m.i32(32);
    let high = s.m.bin(b, Bin::Add, lane, k32);
    let lanes = [lane, high];
    let Some((slot, shared)) = counter else {
        return (lanes, lanes);
    };
    let lds = s.lds();
    let (k, zero) = (s.m.i32(slot), s.m.i32(0));
    let at = s.m.element(b, lds, k);
    s.m.store(b, at, zero);
    // (DXIL barrier mode 9: the group waits, with a thread-group shared memory fence)
    let mode = s.m.i32(9);
    let void = s.t.void;
    s.op(b, "barrier", Attr::NoDuplicate, void, 80, &[mode]);
    let first = s.m.cmp(b, pred::EQ, lane, zero);
    let k64 = s.m.i32(64);
    let add = s.m.select(b, first, k64, zero);
    let at = s.m.element(b, lds, k);
    let ticket = s.m.atomic(b, Rmw::Add, at, add);
    let base = s.op(
        b,
        "waveReadLaneFirst.i32",
        Attr::NoUnwind,
        i32t,
        118,
        &[ticket],
    );
    if shared {
        s.op(b, "barrier", Attr::NoDuplicate, void, 80, &[mode]);
    }
    let flat = lanes.map(|l| s.m.bin(b, Bin::Add, base, l));
    (lanes, flat)
}

/// The threads of a console group in the paired form: whole console waves, each one host wave
/// of 32 (a host group holds at most 1,024 threads). Numbering several needs a group barrier
/// that waits (not with `VICTORY_GROUP_SYNC=0`).
fn group_threads(shape: [u32; 3], waits: bool) -> Result<u32> {
    let group = shape
        .into_iter()
        .try_fold(1u32, u32::checked_mul)
        .ok_or("paired: a thread group past counting")?;
    if group == 0 || group > 1024 || !group.is_multiple_of(64) {
        return Err("paired: a thread group that is not whole console waves");
    }
    if group > 64 && !waits {
        return Err("paired: several console waves in a group, without group barriers that wait");
    }
    Ok(group)
}

/// Rewrites `s` (lowered as for a host wave of 64 lanes, the console's) in the paired form.
/// `regions`: the DXIL blocks where an instruction's code that branches per lane starts and
/// joins again; `tails`: the block each IR block ends in, whose branch is the console wave's
/// (wave-wide or on a scalar); `counter`: the LDS dword numbering the host waves of a group of
/// several console waves.
pub(crate) fn run(
    s: &mut Shader,
    regions: &[(usize, usize)],
    tails: &[Option<usize>],
    counter: Option<(u32, bool)>,
) -> Result<()> {
    let shape = s.threads.ok_or("paired: not a compute program")?;
    let group = group_threads(shape, crate::emit::group_sync() != 0)?;
    if counter.is_some() != (group > 64) {
        return Err("paired: no counter for the waves of the group");
    }
    let old = std::mem::take(&mut s.m.blocks);
    let lowered = std::mem::take(&mut s.m.insts);
    s.m.tags.clear();
    let mut values = Values(vec![None; lowered.len()]);
    s.entry = s.m.new_block();
    let (lanes, flat) = thread_numbers(s, counter);
    let dest = Blocks::new(&mut s.m, &old, regions)?;
    s.m.terminate(s.entry, Term::Br(dest.ids[0][0]));
    let mut decided = vec![false; old.len()];
    for &b in tails.iter().flatten() {
        decided[b] = true;
    }
    // what a call of each function does (the declarations made below are pure arithmetic)
    let effects: Vec<opt::Effect> = s.m.funcs.iter().map(opt::effect).collect();
    // (the opcode of each `dx.op` call)
    let opcode_of: Vec<Option<i64>> = lowered
        .iter()
        .map(|inst| match &inst.op {
            Op::Call(_, args) => args.first().and_then(|&a| s.m.int_value(a)),
            _ => None,
        })
        .collect();
    let is_ballot = |v: V| matches!(v, V::Inst(i) if opcode_of[i as usize] == Some(116));
    let mut joins = Vec::new();
    for b in order(&old) {
        let block = &old[b];
        // The console's branches take one way for the whole wave; a branch of one
        // instruction's own code may take another way in each lane.
        if matches!(block.term, Some(Term::CondBr(..)))
            && !decided[b]
            && dest.starts[b].is_none()
            && dest.inside[b].is_none()
        {
            return Err("paired: a branch per lane outside an instruction's own code");
        }
        let ids = dest.ids[b];
        for &i in &block.phis {
            let ty = lowered[i as usize].ty;
            let low = dest.ends[b].map_or(ids[0], |r| r.middle);
            let pair = [s.m.phi(low, ty), s.m.phi(ids[1], ty)];
            values.0[i as usize] = Some(pair);
            joins.push((i, pair));
        }
        for &i in &block.body {
            let Inst { ty, op } = lowered[i as usize].clone();
            let special = match &op {
                Op::Call(f, args) => {
                    let opcode =
                        opcode_of[i as usize].ok_or("paired: a call that is not a dx.op")?;
                    match opcode {
                        // waveGetLaneIndex, waveGetLaneCount, flattenedThreadIdInGroup
                        111 => Some(lanes),
                        112 => Some([s.m.i32(64); 2]),
                        96 => Some(flat),
                        // threadIdInGroup, of the console's group shape
                        95 => {
                            let dim = args
                                .get(1)
                                .and_then(|&a| s.m.int_value(a))
                                .ok_or("paired: a thread id of no constant dimension")?;
                            let dim = usize::try_from(dim)
                                .ok()
                                .filter(|&d| d < 3)
                                .ok_or("paired: a thread id of no dimension")?;
                            let stride = s.m.i32(shape[..dim].iter().product());
                            let size = s.m.i32(shape[dim]);
                            Some(std::array::from_fn(|k| {
                                let at = s.m.bin(ids[k], Bin::UDiv, flat[k], stride);
                                s.m.bin(ids[k], Bin::URem, at, size)
                            }))
                        }
                        93 => return Err("paired: a dispatch thread id"),
                        // a group barrier or fence: once, for the host wave
                        80 if dest.shared(b) => {
                            Some([s.m.call(ids[0], V::Func(*f), args.clone()); 2])
                        }
                        80 => return Err("paired: a group barrier inside one instruction's code"),
                        110 | 113..=123 | 135 | 136 | 165..=167 if dest.shared(b) => {
                            Some(wave(s, ids[0], *f, opcode, args, &values)?)
                        }
                        110 | 113..=123 | 135 | 136 | 165..=167 => {
                            return Err("paired: a wave operation inside one instruction's code");
                        }
                        _ => None,
                    }
                }
                // a word of a ballot: the low one is bank 0's ballot, the high one bank 1's
                Op::Extract(a, field) if is_ballot(*a) => {
                    let halves = values.get(*a)?;
                    Some(std::array::from_fn(|k| {
                        if *field < 2 {
                            s.m.extract(ids[k], halves[*field as usize], 0)
                        } else {
                            s.m.i32(0)
                        }
                    }))
                }
                _ => None,
            };
            let mut read_ballot = false;
            if special.is_none() {
                for_each_operand(&op, |v| read_ballot |= is_ballot(v));
            }
            if read_ballot {
                return Err("paired: a ballot read other than by its words");
            }
            let pair = match special {
                Some(pair) => pair,
                None => {
                    let pure = match &op {
                        Op::Bin(..)
                        | Op::Cast(..)
                        | Op::Cmp(..)
                        | Op::Select(..)
                        | Op::Extract(..)
                        | Op::Gep(..)
                        | Op::Load(_) => true,
                        Op::Call(f, _) => effects
                            .get(*f as usize)
                            .is_some_and(|e| matches!(e, opt::Effect::Pure | opt::Effect::Reads)),
                        _ => false,
                    };
                    if pure && dest.shared(b) && values.alike(&op) {
                        // the same in both halves: written once
                        let v = s.m.push(ids[0], ty, values.operation(&op, 0)?);
                        [v; 2]
                    } else {
                        [
                            s.m.push(ids[0], ty, values.operation(&op, 0)?),
                            s.m.push(ids[1], ty, values.operation(&op, 1)?),
                        ]
                    }
                }
            };
            values.0[i as usize] = Some(pair);
        }
        let banks = if dest.starts[b].is_some() || dest.inside[b].is_some() {
            2
        } else {
            1
        };
        for k in 0..banks {
            let target = |next| dest.to(b, next, k);
            let term = match block.term.ok_or("paired: a block without a terminator")? {
                Term::Br(t) => Term::Br(target(t)),
                Term::CondBr(c, t, f) => Term::CondBr(values.get(c)?[k], target(t), target(f)),
                Term::Ret => Term::Ret,
            };
            s.m.terminate(dest.from(b, k), term);
        }
    }
    for (i, pair) in joins {
        let Op::Phi(edges) = &lowered[i as usize].op else {
            unreachable!("a join")
        };
        for (k, phi) in pair.into_iter().enumerate() {
            let edges = edges
                .iter()
                .map(|&(v, pred)| Ok((values.get(v)?[k], dest.from(pred, k))))
                .collect::<Result<Vec<_>>>()?;
            s.m.set_phi(phi, edges);
        }
    }
    for (_, handle) in s
        .root_cbvs
        .iter_mut()
        .chain(&mut s.root_srvs)
        .chain(&mut s.root_uavs)
    {
        *handle = values.get(*handle)?[0];
    }
    // (the thread ids replaced leave declarations nothing calls, which the validator refuses)
    s.m.drop_unused_functions();
    s.flags |= flags::WAVE_OPS;
    s.wave_size = Some(32);
    s.threads = Some([group / 2, 1, 1]);
    s.entry = 0;
    Ok(())
}

/// Wave operation `opcode` (a call of `f` with `args`) over both banks, in block `b` (one both
/// banks share: every host lane runs it).
fn wave(s: &mut Shader, b: usize, f: u32, opcode: i64, args: &[V], vals: &Values) -> Result<Pair> {
    let arg = |i: usize| {
        args.get(i)
            .ok_or("paired: a wave operation's missing operand")
            .and_then(|&v| vals.get(v))
    };
    let call = |s: &mut Shader, k: usize| -> Result<V> {
        let operands = args
            .iter()
            .map(|&v| Ok(vals.get(v)?[k]))
            .collect::<Result<Vec<_>>>()?;
        Ok(s.m.call(b, V::Func(f), operands))
    };
    Ok(match opcode {
        // waveIsFirstLane: the host wave's first lane is console lane 0
        110 => [call(s, 0)?, s.m.bool(false)],
        // waveAnyTrue, waveAllTrue
        113 | 114 => {
            let halves = [call(s, 0)?, call(s, 1)?];
            let join = if opcode == 113 { Bin::Or } else { Bin::And };
            [s.m.bin(b, join, halves[0], halves[1]); 2]
        }
        // waveActiveBallot (its words: `run`), quadReadLaneAt, quadOp: within each half
        116 | 122 | 123 => [call(s, 0)?, call(s, 1)?],
        // waveReadLaneAt: lanes 0..31 are bank 0's, 32..63 bank 1's
        117 => {
            let src = arg(1)?;
            let indices = arg(2)?;
            let k31 = s.m.i32(31);
            let mut read = |index: V| {
                let lane = s.m.bin(b, Bin::And, index, k31);
                let lower = s.m.cmp(b, pred::IULE, index, k31);
                let halves = src.map(|v| s.m.call(b, V::Func(f), vec![args[0], v, lane]));
                s.m.select(b, lower, halves[0], halves[1])
            };
            [read(indices[0]), read(indices[1])]
        }
        // waveReadLaneFirst: every host lane runs here, so the first is console lane 0
        118 => [call(s, 0)?; 2],
        // waveActiveOp, waveActiveBit: both halves' results combined; wavePrefixOp: bank 1's
        // prefix after the whole of bank 0
        119..=121 => {
            let halves = [call(s, 0)?, call(s, 1)?];
            let kind = args
                .get(2)
                .and_then(|&a| s.m.int_value(a))
                .ok_or("paired: a wave operation of no constant kind")?;
            let ty = s.m.type_of(halves[0]);
            let left = if opcode == 121 {
                let name = s.m.funcs[f as usize]
                    .name
                    .replace("dx.op.wavePrefixOp.", "waveActiveOp.");
                let sign = *args
                    .get(3)
                    .ok_or("paired: a wave prefix without its sign")?;
                s.op(
                    b,
                    &name,
                    Attr::NoUnwind,
                    ty,
                    119,
                    &[arg(1)?[0], args[2], sign],
                )
            } else {
                halves[0]
            };
            let right = halves[1];
            let joined = match (opcode, kind) {
                (120, 0) => s.m.bin(b, Bin::And, left, right),
                (120, 1) => s.m.bin(b, Bin::Or, left, right),
                (120, 2) => s.m.bin(b, Bin::Xor, left, right),
                (120, _) => return Err("paired: an unknown wave bit operation"),
                (_, 0) => s.m.bin(b, Bin::Add, left, right),
                (_, 1) => s.m.bin(b, Bin::Mul, left, right),
                (_, 2 | 3) => {
                    let unsigned = args.get(3).and_then(|&a| s.m.int_value(a)) == Some(1);
                    // FMax 35, FMin 36, IMax 37, IMin 38, UMax 39, UMin 40
                    let (name, min, max) = match s.m.types[ty.0 as usize] {
                        TypeDef::Float => ("binary.f32", 36, 35),
                        TypeDef::Double => ("binary.f64", 36, 35),
                        TypeDef::Int(32) if unsigned => ("binary.i32", 40, 39),
                        TypeDef::Int(32) => ("binary.i32", 38, 37),
                        _ => return Err("paired: a wave minimum or maximum of an unknown type"),
                    };
                    s.op(
                        b,
                        name,
                        Attr::ReadNone,
                        ty,
                        if kind == 2 { min } else { max },
                        &[left, right],
                    )
                }
                _ => return Err("paired: an unknown wave operation"),
            };
            if opcode == 121 {
                [halves[0], joined]
            } else {
                [joined; 2]
            }
        }
        _ => return Err("paired: a wave operation the paired form does not take"),
    })
}

#[cfg(test)]
#[test]
fn a_group_is_paired_in_whole_console_waves_with_barriers_that_wait() {
    assert_eq!(group_threads([8, 8, 1], false), Ok(64));
    assert_eq!(group_threads([8, 8, 2], true), Ok(128));
    assert_eq!(group_threads([1024, 1, 1], true), Ok(1024));
    assert!(group_threads([8, 8, 2], false).is_err());
    for shape in [
        [48, 1, 1],
        [96, 1, 1],
        [0, 1, 1],
        [2048, 1, 1],
        [u32::MAX, 2, 1],
    ] {
        assert!(group_threads(shape, true).is_err(), "{shape:?}");
    }
}
