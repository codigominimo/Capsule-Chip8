//! Verificación headless: comandos de disco, persistencia y crecimiento real.
//! Uso: cargo run --release --example selftest -- <exe generado>

use capsule_core::capsule::CapsuleFile;
use capsule_core::chip8::{CMD_GROW, CMD_READ, CMD_SIZE, CMD_WRITE, Chip8};
use std::path::{Path, PathBuf};

fn main() {
    let path = PathBuf::from(
        std::env::args()
            .nth(1)
            .expect("falta la ruta del exe generado"),
    );
    let file = CapsuleFile::open(&path).expect("no puedo abrir la cápsula");
    let state = file.load_state();
    println!("ROM: {} bytes, disco: {} bytes", file.rom().len(), file.disk_len());
    let initial_sectors = (file.disk_len() / 16) as u16;
    let mut chip = Chip8::new(state, file);

    let expected: [u8; 16] = core::array::from_fn(|i| (i as u8).wrapping_mul(7).wrapping_add(1));
    chip.write_mem(0x100, 0);
    chip.write_mem(0x101, 0);
    for (i, b) in expected.iter().enumerate() {
        chip.write_mem(0x102 + i, *b);
    }
    chip.write_mem(0x112, CMD_WRITE);
    assert_eq!(chip.state.ram[0x113], 0, "CMD_WRITE falló");

    chip.write_mem(0x112, CMD_SIZE);
    assert_eq!(
        u16::from_le_bytes([chip.state.ram[0x102], chip.state.ram[0x103]]),
        initial_sectors,
        "CMD_SIZE debe devolver el número de sectores"
    );

    chip.write_mem(0x100, 0);
    chip.write_mem(0x101, 0);
    for i in 0..16 {
        chip.write_mem(0x102 + i, 0);
    }
    chip.write_mem(0x112, CMD_READ);
    assert_eq!(chip.state.ram[0x113], 0, "CMD_READ falló");
    assert_eq!(&chip.state.ram[0x102..0x112], &expected, "el sector leído no coincide");

    let before = file_size(&path);
    chip.write_mem(0x112, CMD_GROW);
    assert_eq!(chip.state.ram[0x113], 0, "CMD_GROW falló");
    let after = file_size(&path);
    assert!(after > before, "el exe no creció físicamente");

    drop(chip);
    let again = CapsuleFile::open(&path).expect("la cápsula quedó corrupta tras crecer");
    assert_eq!(
        again.disk_len(),
        (initial_sectors as usize + 256) * 16,
        "el disco no creció un chunk"
    );
    assert_eq!(again.read_sector(0).unwrap(), expected, "el dato no persistió");

    println!("OK: disco {before} -> {after} bytes, sector 0 persistido, snapshot válido");
}

fn file_size(p: &Path) -> u64 {
    std::fs::metadata(p).unwrap().len()
}
