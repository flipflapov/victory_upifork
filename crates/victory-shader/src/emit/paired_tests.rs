//! The paired form (`bitcode::paired`): small console wave64 compute programs, each host lane
//! carrying two console lanes, run on the host GPU when it takes waves of 32, and compared
//! with what the console gives.

use super::*;

/// `native` in the paired form (`super::super::emit_paired`), validated by `dxil.dll` when
/// present.
fn paired(native: &NativeProgram) -> HostShader {
    let p = ir::build(&native.code, native.wave64, native.vgprs).expect("ir::build");
    let st = structure::structure(&p).expect("structure");
    let bytes = super::super::container::build(&[]);
    let c = super::super::Container::parse(&bytes).expect("container");
    let was = lower::TEST_HOST_WAVE.replace(Some(32));
    let r = super::super::emit_paired(&c, native, &p, &st);
    lower::TEST_HOST_WAVE.set(was);
    let (shader, wave) = r.expect("the paired form");
    assert_eq!(wave, Some(32));
    if let Ok(mut v) = validator::Validator::load(validator::Validator::default_path().as_deref()) {
        let mut copy = shader.dxil.clone();
        v.validate(&mut copy).expect("DXIL validation failed");
    }
    shader
}

/// The first `words` dwords of the UAV after one group of `shader` ran with root constant s0
/// = `s0`; none without a device that takes it.
fn run(host: &Host, shader: &HostShader, s0: u32, words: u32) -> Option<Vec<u32>> {
    let (root, pso) = match host.compute_pipeline(shader) {
        Ok(x) => x,
        Err(e) => {
            eprintln!(
                "the hardware device does not take this program (waves of 32?): {e}; not run"
            );
            return None;
        }
    };
    let mut ud = vec![0u32; shader.user_data_dwords().max(1) as usize];
    for u in &shader.user_data {
        if let (RootType::Constants, 0, 0) = (u.root, u.slot, u.dword) {
            ud[u.index as usize] = s0;
        }
    }
    let out = host
        .dispatch(
            &root,
            &pso,
            &Dispatch {
                roots: vec![(0, Root::Constants(ud))],
                views: &[View::RawUav(words)],
                samplers: &[],
                groups: [1, 1, 1],
            },
        )
        .unwrap_or_else(|e| panic!("dispatch failed: {e}"));
    Some(out[0].clone())
}

/// The host GPU, if there is one that runs SM 6.6.
fn card() -> Option<Host> {
    match Host::new(false) {
        Ok(host) => Some(host),
        Err(e) => {
            eprintln!("no SM 6.6 hardware device ({e}); not run");
            None
        }
    }
}

/// A wave OR inside `if (t < s0)` (each thread's bit `1 << (t >> 1)`), stored by the threads
/// inside: the console ORs only the lanes in EXEC, from both halves of the wave, whichever
/// half they are in. The other exact forms refuse it ("a wave reduction where not every lane
/// runs"); here no barrier is needed for it.
#[test]
fn a_wave_reduction_with_only_some_lanes_in_exec_is_the_consoles() {
    let shader = paired(&program(0xBF88_0000));
    let _one = super::super::valu_tests::ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let Some(host) = card() else { return };
    for below in [0u32, 1, 2, 10, 31, 32, 33, 40, 63, 64] {
        let expected = (0..below).fold(0u32, |bits, t| bits | 1 << (t >> 1));
        let Some(out) = run(&host, &shader, below, 1) else {
            return;
        };
        assert_eq!(out, [expected], "{below} threads in the branch");
    }
    eprintln!("ran on the hardware device: the OR of the lanes in EXEC, in every case");
}

/// `v_readfirstlane` inside `if (t > s0)`: the console reads the first lane in EXEC (s0 + 1,
/// in either half); every lane there adds it to the value `ds_bpermute` read from the lane in
/// the other half (`t ^ 32`).
#[test]
fn a_first_lane_read_with_only_some_lanes_in_exec_and_a_lane_read_across_halves() {
    let shader = paired(&native(&[
        0x3402_0082, // v_lshlrev_b32 v1, 2, v0
        0x3A04_00A0, // v_xor_b32 v2, 32, v0
        0x3404_0482, // v_lshlrev_b32 v2, 2, v2
        0xDACC_0000,
        0x0300_0002, // ds_bpermute_b32 v3, v2, v0
        0x7D82_0000, // v_cmp_lt_u32 vcc, s0, v0
        0xBE8C_246A, // s_and_saveexec_b64 s[12:13], vcc
        0xBF88_0004, // s_cbranch_execz to the restore
        0x7E0E_0500, // v_readfirstlane_b32 s7, v0
        0x4A06_0607, // v_add_nc_u32 v3, s7, v3
        0xE070_1000,
        0x8002_0301, // buffer_store_dword v3, v1, s[8:11], 0 offen
        0xBEFE_040C, // s_mov_b64 exec, s[12:13]
    ]));
    let _one = super::super::valu_tests::ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let Some(host) = card() else { return };
    for above in [0u32, 10, 30, 31, 32, 40, 54, 62, 63] {
        let expected: Vec<u32> = (0..64)
            .map(|t| if t > above { (t ^ 32) + above + 1 } else { 0 })
            .collect();
        let Some(out) = run(&host, &shader, above, 64) else {
            return;
        };
        assert_eq!(out, expected, "lanes above {above}");
    }
    eprintln!(
        "ran on the hardware device: the first lane in EXEC and the other half's lanes, in every case"
    );
}

/// A lane mask as data (`vcc` of `t < s0`, both dwords) and `v_readlane` of lane 43 (in the
/// upper half), stored by every lane.
#[test]
fn a_lane_mask_as_data_and_a_lane_read_from_the_upper_half() {
    let shader = paired(&native(&[
        0x7D88_0000, // v_cmp_gt_u32 vcc, s0, v0
        0x7E08_026A, // v_mov_b32 v4, vcc_lo
        0x7E0A_026B, // v_mov_b32 v5, vcc_hi
        0xD760_0007,
        0x0001_5700, // v_readlane_b32 s7, v0, 43
        0x7E0C_0207, // v_mov_b32 v6, s7
        0xE070_0000,
        0x8002_0400, // buffer_store_dword v4, off, s[8:11], 0
        0xE070_0004,
        0x8002_0500, // buffer_store_dword v5, off, s[8:11], 0 offset:4
        0xE070_0008,
        0x8002_0600, // buffer_store_dword v6, off, s[8:11], 0 offset:8
    ]));
    let _one = super::super::valu_tests::ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let Some(host) = card() else { return };
    for below in [0u32, 1, 31, 32, 33, 50, 64] {
        let mask = if below == 64 {
            u64::MAX
        } else {
            (1u64 << below) - 1
        };
        let expected = vec![mask as u32, (mask >> 32) as u32, 43];
        let Some(out) = run(&host, &shader, below, 3) else {
            return;
        };
        assert_eq!(out, expected, "{below} lanes in the mask");
    }
    eprintln!("ran on the hardware device: the mask's two dwords and lane 43, in every case");
}

/// Each thread writes its id to LDS dword t, the group meets (`s_barrier`), and the threads
/// below s0 read the dword of thread `t ^ (threads / 2)` – the other half of their console
/// wave with one wave, the other console wave with two – and add lane 43 of their own wave
/// (`v_readlane`; `late`: read inside the branch, where not every lane runs). The console's
/// lanes of one wave see each other's writes without a barrier.
fn shared(threads: u32, late: bool) -> NativeProgram {
    let read_lane = [
        0xD760_0007,
        0x0001_5700, // v_readlane_b32 s7, v0, 43
    ];
    let mut body = vec![
        0x3402_0082, // v_lshlrev_b32 v1, 2, v0
        0xD834_0000,
        0x0000_0001,                                         // ds_write_b32 v1, v0
        0xBF8A_0000,                                         // s_barrier
        0x3A04_0000 | if threads == 64 { 160 } else { 192 }, // v_xor_b32 v2, threads / 2, v0
        0x3404_0482,                                         // v_lshlrev_b32 v2, 2, v2
    ];
    if !late {
        body.extend(read_lane);
    }
    body.extend([
        0x7D88_0000,                       // v_cmp_gt_u32 vcc, s0, v0
        0xBE8C_246A,                       // s_and_saveexec_b64 s[12:13], vcc
        0xBF88_0005 + 2 * u32::from(late), // s_cbranch_execz to the restore
    ]);
    if late {
        body.extend(read_lane);
    }
    body.extend([
        0xD8D8_0000,
        0x0300_0002, // ds_read_b32 v3, v2
        0x4A06_0607, // v_add_nc_u32 v3, s7, v3
        0xE070_1000,
        0x8002_0301, // buffer_store_dword v3, v1, s[8:11], 0 offen
        0xBEFE_040C, // s_mov_b64 exec, s[12:13]
    ]);
    let mut native = native_of(&body, true, threads);
    let StageIo::Cs(ref mut cs) = native.io else {
        unreachable!()
    };
    cs.rsrc2 |= 1 << 15;
    assert!(cs.lds_bytes() >= threads * 4);
    native
}

#[test]
fn an_lds_write_is_read_by_other_lanes_and_waves_and_divergent_stores_stay_masked() {
    let _one = super::super::valu_tests::ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let host = card();
    for (threads, late) in [(64u32, false), (128, false), (64, true), (128, true)] {
        let shader = paired(&shared(threads, late));
        let Some(host) = &host else { continue };
        for below in [0u32, 17, 32, 47, 64, 83, 128] {
            let expected: Vec<u32> = (0..=threads)
                .map(|t| {
                    if t < threads && t < below {
                        (t ^ (threads / 2)) + (t & !63) + 43
                    } else {
                        0
                    }
                })
                .collect();
            let Some(out) = run(host, &shader, below, threads + 1) else {
                return;
            };
            assert_eq!(
                out, expected,
                "{threads} threads, those below {below} reading, lane read inside the branch: {late}"
            );
        }
        eprintln!(
            "ran on the hardware device: LDS between lanes and waves, {threads} threads, lane read inside the branch: {late}"
        );
    }
}

/// A wave OR and a first-lane read inside `if (t >= s0)`: with s0 = 32 only the upper half
/// of the console wave is in the branch, the first lane in EXEC is 32 and the low dword of
/// every mask is 0; with s0 = 64 no lane is, and nothing is stored.
#[test]
fn a_branch_only_one_half_of_the_console_wave_is_in() {
    let mut code = vec![
        0x7E0A_0281,      // v_mov_b32 v5, 1
        0x2C0C_0081,      // v_lshrrev_b32 v6, 1, v0
        0x340E_0B06,      // v_lshlrev_b32 v7, v6, v5
        0x7D86_0000,      // v_cmp_le_u32 vcc, s0, v0
        0xBE8C_246A,      // s_and_saveexec_b64 s[12:13], vcc
        0xBF88_0000 | 27, // s_cbranch_execz to the restore
    ];
    code.extend_from_slice(&super::super::wave::WAVE_OR_V7_TO_S7);
    code.extend_from_slice(&[
        0x7E0C_0500, // v_readfirstlane_b32 s6, v0
        0x7E00_0207, // v_mov_b32 v0, s7
        0x7E08_0206, // v_mov_b32 v4, s6
        0xE070_0000,
        0x8002_0000, // buffer_store_dword v0, off, s[8:11], 0
        0xE070_0004,
        0x8002_0400, // buffer_store_dword v4, off, s[8:11], 0 offset:4
        0xBEFE_040C, // s_mov_b64 exec, s[12:13]
    ]);
    let shader = paired(&native(&code));
    let _one = super::super::valu_tests::ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let Some(host) = card() else { return };
    for from in [0u32, 1, 31, 32, 33, 40, 63, 64] {
        let expected = if from < 64 {
            vec![(from..64).fold(0u32, |bits, t| bits | 1 << (t >> 1)), from]
        } else {
            vec![0, 0]
        };
        let Some(out) = run(&host, &shader, from, 2) else {
            return;
        };
        assert_eq!(out, expected, "the lanes from {from} in the branch");
    }
    eprintln!("ran on the hardware device: a branch of one half, in every case");
}

/// `do { s1 += WaveActiveBitOr(1 << (t >> 1)); s0 -= 1; } while (t < s0)` in a group of
/// `threads`, then s1 stored: a loop the lanes leave one by one, around a wave OR. Exact only
/// with the loop's branch taken by the wave, which a group of several console waves cannot
/// have in two host waves each (a barrier inside it would wait for the other waves).
fn leave_one_by_one(threads: u32) -> NativeProgram {
    let mut code = vec![
        0x7E0A_0281, // v_mov_b32 v5, 1
        0x2C0C_0081, // v_lshrrev_b32 v6, 1, v0
        0x340E_0B06, // v_lshlrev_b32 v7, v6, v5
        0xBE8C_047E, // s_mov_b64 s[12:13], exec
    ];
    code.extend_from_slice(&super::super::wave::WAVE_OR_V7_TO_S7);
    code.extend_from_slice(&[
        0x8001_0701,          // s_add_u32 s1, s1, s7
        0x8080_8100,          // s_sub_u32 s0, s0, 1
        0x7DA8_0000,          // v_cmpx_gt_u32 exec, s0, v0
        0xBF89_0000 | 0xFFE8, // s_cbranch_execnz to the wave OR (24 words back)
        0xBEFE_040C,          // s_mov_b64 exec, s[12:13]
        0x7E00_0201,          // v_mov_b32 v0, s1
        0x38 << 26 | 0x1C << 18,
        0x80 << 24 | 2 << 16, // buffer_store_dword v0, off, s[8:11], 0
    ]);
    native_of(&code, true, threads)
}

/// `if (t < s0) s7 = ReadFirstLane(t)` (s7 = 0 before), then every thread stores s7 plus lane
/// 40 of its wave to dword t, in a group of `threads`: the branch cannot be left out (the code
/// after it reads the scalar it set), and the console wave's first lane in EXEC may be in
/// either half. (The lane read across the halves makes the two host waves of a console wave
/// exchange; without one, each host wave reads its own first lane.)
fn first_lane_kept(threads: u32) -> NativeProgram {
    native_of(
        &[
            0xD760_0006,
            0x0001_5100, // v_readlane_b32 s6, v0, 40 (a lane read: the halves exchange)
            0xBE87_0380, // s_mov_b32 s7, 0
            0x3402_0082, // v_lshlrev_b32 v1, 2, v0
            0x7D88_0000, // v_cmp_gt_u32 vcc, s0, v0
            0xBE8C_246A, // s_and_saveexec_b64 s[12:13], vcc
            0xBF88_0001, // s_cbranch_execz over the next
            0x7E0E_0500, // v_readfirstlane_b32 s7, v0
            0xBEFE_040C, // s_mov_b64 exec, s[12:13]
            0x7E06_0207, // v_mov_b32 v3, s7
            0x4A06_0606, // v_add_nc_u32 v3, s6, v3
            0xE070_1000,
            0x8002_0301, // buffer_store_dword v3, v1, s[8:11], 0 offen
        ],
        true,
        threads,
    )
}

/// The switch picks the form: a program named ([`super::super::with_paired_first`], as
/// `VICTORY_WAVE_PAIRED_FOR` does) takes the paired form before any other, the wave OR in a
/// branch of one console wave ([`program`]) too, which has an exact form without it;
/// [`first_lane_kept`] in a group of two console waves, approximate without it (each host wave
/// on its own, every mask and lane read of its own), gives the console's first lane on the card
/// with it. So does [`leave_one_by_one`] with one console wave, round by round.
#[test]
fn the_paired_form_is_taken_when_asked_for() {
    let emit = |native: &NativeProgram, paired: bool| {
        let p = ir::build(&native.code, native.wave64, native.vgprs).expect("ir::build");
        let st = structure::structure(&p).expect("structure");
        let res =
            resource::track(&p, &native.layout, native.stage, Some(&st)).expect("resource::track");
        let io = io::raise(&p, native).expect("io::raise");
        let bytes = super::super::container::build(&[]);
        let c = super::super::Container::parse(&bytes).expect("container");
        let was = lower::TEST_HOST_WAVE.replace(Some(32));
        let r = super::super::with_paired_first(paired, || {
            super::super::emit(&c, native, &p, &st, &io, &res)
        });
        lower::TEST_HOST_WAVE.set(was);
        r.expect("emit").dxil
    };
    let exact = program(0xBF88_0000);
    // (named: the paired form even where an exact one came first)
    assert_ne!(emit(&exact, false), paired(&exact).dxil);
    assert_eq!(emit(&exact, true), paired(&exact).dxil);
    let approximate = first_lane_kept(128);
    let before = emit(&approximate, false);
    assert_ne!(before, emit(&approximate, true));
    assert_eq!(emit(&approximate, true), paired(&approximate).dxil);
    assert_eq!(emit(&approximate, false), before);
    let _one = super::super::valu_tests::ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let Some(host) = card() else { return };
    let shader = paired(&approximate);
    for s0 in [0u32, 1, 20, 33, 63, 64, 65, 100, 128] {
        let expected: Vec<u32> = (0..128u32)
            .map(|t| if t & !63 < s0 { t & !63 } else { 0 } + (t & !63) + 40)
            .collect();
        let Some(out) = run(&host, &shader, s0, 128) else {
            return;
        };
        assert_eq!(out, expected, "threads below {s0} in the branch");
    }
    let shader = paired(&leave_one_by_one(64));
    for s0 in [1u32, 2, 20, 33, 40, 64, 70] {
        // the console: every lane in the first round, then those below s0 as it comes down
        let (mut s1, mut s, mut active) = (0u32, s0, [true; 64]);
        loop {
            let or = (0..64u32)
                .filter(|&t| active[t as usize])
                .fold(0u32, |bits, t| bits | 1 << (t >> 1));
            s1 = s1.wrapping_add(or);
            s = s.wrapping_sub(1);
            for t in 0..64u32 {
                active[t as usize] &= t < s;
            }
            if !active.contains(&true) {
                break;
            }
        }
        let Some(out) = run(&host, &shader, s0, 1) else {
            return;
        };
        assert_eq!(out, [s1], "s0 = {s0}");
    }
    eprintln!(
        "ran on the hardware device: the first lane of two console waves, and the loop's rounds, in every case"
    );
}
