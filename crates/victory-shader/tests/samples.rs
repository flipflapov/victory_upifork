//! Containers taken from a run (`VICTORY_BLOBS_LOG=1`, then decoded), translated
//! offline as on a host of 32-lane waves: `VICTORY_SAMPLES=<dir>` names a folder
//! of `.cdxil` files (they are the title's: never in this repository). Ignored
//! without it.

#[test]
fn samples_translate_on_32_lane_hosts() {
    let Some(dir) = std::env::var_os("VICTORY_SAMPLES") else {
        return;
    };
    victory_shader::emit::set_host_max_wave(32);
    let mut failed = Vec::new();
    let mut entries: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.path()).collect();
    entries.sort();
    for path in entries.iter().filter(|p| p.extension().is_some_and(|e| e == "cdxil")) {
        let bytes = std::fs::read(path).unwrap();
        match victory_shader::emit::translate(&bytes) {
            Ok(shader) => eprintln!("{}: translated, {} bytes of DXIL", path.display(), shader.dxil.len()),
            Err(e) => {
                eprintln!("{}: {e}", path.display());
                failed.push(path.clone());
            }
        }
    }
    assert!(failed.is_empty(), "{} of the samples do not translate: {failed:?}", failed.len());
}

/// `VICTORY_SAMPLE_DISASM=<file>`: the compute, vertex, pixel or mesh program of one
/// container, disassembled to stderr (for working on a translator gap offline).
#[test]
fn sample_disassembly() {
    let Some(path) = std::env::var_os("VICTORY_SAMPLE_DISASM") else {
        return;
    };
    let bytes = std::fs::read(path).unwrap();
    let c = victory_shader::Container::parse(&bytes).unwrap();
    let dxil = c.part(victory_shader::FourCC::DXIL).or(c.part(victory_shader::FourCC::ACCC)).unwrap();
    for p in victory_shader::native_programs(dxil.data).unwrap() {
        eprintln!("program {:?} wave64={} vgprs={} io={:?}", p.stage, p.wave64, p.vgprs, p.io);
        for inst in victory_shader::gfx10::decode(&p.code).unwrap() {
            eprintln!("{:#06x}  {}", inst.offset, inst.text(p.wave64));
        }
    }
}
