//! Verificación headless: el estado de la máquina persiste en el exe.
//! Uso: cargo run --release --example selftest -- <exe generado>

use capsule_core::capsule::CapsuleFile;
use capsule_core::chip8::Chip8;
use std::path::PathBuf;

fn main() {
    let path = PathBuf::from(
        std::env::args()
            .nth(1)
            .expect("falta la ruta del exe generado"),
    );
    let size_before = std::fs::metadata(&path).unwrap().len();

    let mut file = CapsuleFile::open(&path).expect("no puedo abrir la cápsula");
    println!("ROM: {} bytes", file.rom().len());

    let mut chip = Chip8::new(file.load_state());
    chip.state.v[0] = 0xab;
    chip.state.v[0xf] = 0x5a;
    chip.state.pc = 0x234;
    chip.state.i = 0x3a1;
    chip.state.sp = 2;
    chip.state.stack[2] = 0x456;
    chip.state.delay = 17;
    chip.state.sound = 9;
    chip.state.ram[0x300] = 0x7e;
    chip.state.gfx[17] = true;
    chip.state.gfx[42] = true;
    file.checkpoint(&chip.state);

    let again = CapsuleFile::open(&path).expect("la cápsula quedó corrupta");
    let s = again.load_state();
    assert_eq!(s.v[0], 0xab);
    assert_eq!(s.v[0xf], 0x5a);
    assert_eq!(s.pc, 0x234);
    assert_eq!(s.i, 0x3a1);
    assert_eq!(s.sp, 2);
    assert_eq!(s.stack[2], 0x456);
    assert_eq!(s.delay, 17);
    assert_eq!(s.sound, 9);
    assert_eq!(s.ram[0x300], 0x7e);
    assert!(s.gfx[17] && s.gfx[42]);
    assert!(!s.gfx[18], "píxel no escrito debe seguir apagado");

    // Segundo checkpoint: debe usar el snapshot alterno sin perder nada.
    chip.state.v[1] = 0xcd;
    chip.state.ram[0x301] = 0x11;
    file.checkpoint(&chip.state);
    let again = CapsuleFile::open(&path).expect("la cápsula quedó corrupta");
    let s = again.load_state();
    assert_eq!(s.v[0], 0xab);
    assert_eq!(s.v[1], 0xcd);
    assert_eq!(s.ram[0x300], 0x7e);
    assert_eq!(s.ram[0x301], 0x11);

    let size_after = std::fs::metadata(&path).unwrap().len();
    assert_eq!(size_before, size_after, "el exe no debe crecer sin disco");

    println!(
        "OK: estado persistido (registros, RAM, stack, timers y GFX), snapshots alternos válidos, exe {} bytes sin cambios",
        size_after
    );
}
