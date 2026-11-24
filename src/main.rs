use capsule_core::audio::Beeper;
use capsule_core::capsule::CapsuleFile;
use capsule_core::chip8::{Chip8, MachineState};
use capsule_core::{launcher, lock, pack};
use minifb::{Key, Window, WindowOptions};
use std::path::{Path, PathBuf};

// Espacio reservado para que el linker cree la sección. `--pack` la reubica al
// final de la copia y sustituye su contenido por la cápsula real.
#[used]
#[unsafe(link_section = ".capsule")]
static CAPSULE_RESERVED: [u8; 16384] = [0; 16384];

const SCALE: usize = 10;
const W: usize = 64 * SCALE;
const H: usize = 32 * SCALE;

fn map_key(k: Key) -> Option<usize> {
    Some(match k {
        Key::Key1 => 0x1,
        Key::Key2 => 0x2,
        Key::Key3 => 0x3,
        Key::Key4 => 0xc,
        Key::Q => 0x4,
        Key::W => 0x5,
        Key::E => 0x6,
        Key::R => 0xd,
        Key::A => 0x7,
        Key::S => 0x8,
        Key::D => 0x9,
        Key::F => 0xe,
        Key::Z => 0xa,
        Key::X => 0x0,
        Key::C => 0xb,
        Key::V => 0xf,
        _ => return None,
    })
}

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn do_pack(args: &[String]) {
    let rom = flag(args, "--rom").or_else(|| flag(args, "-r"));
    let out = flag(args, "--out").or_else(|| flag(args, "-o"));
    let (Some(rom), Some(out)) = (rom, out) else {
        eprintln!("uso: capsule-chip8 --pack --rom <rom.ch8> --out <juego.exe>");
        std::process::exit(1);
    };
    let exe = std::env::current_exe().expect("no puedo resolver la ruta del exe");
    match pack::pack(Path::new(&rom), &exe, Path::new(&out)) {
        Ok(size) => println!("cápsula creada: {out} ({size} bytes)"),
        Err(e) => {
            eprintln!("capsule-chip8: {e}");
            std::process::exit(1);
        }
    }
}

fn run_emulator(path: &Path, reset: bool) {
    let _lock = match lock::InstanceLock::acquire(path) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };

    let mut file = match CapsuleFile::open(path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("cápsula inválida: {e}");
            std::process::exit(1);
        }
    };
    let state = if reset {
        MachineState::new(file.rom())
    } else {
        file.load_state()
    };
    // Así `--reset` queda guardado aunque se cierre antes del primer frame.
    file.checkpoint(&state);
    let mut chip8 = Chip8::new(state);

    let mut window = match Window::new("Capsule-CHIP8", W, H, WindowOptions::default()) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("no puedo abrir la ventana: {e}");
            std::process::exit(1);
        }
    };
    window.set_target_fps(60);
    let mut beeper = match Beeper::new() {
        Ok(b) => Some(b),
        Err(e) => {
            eprintln!("audio no disponible, se continúa sin sonido: {e}");
            None
        }
    };
    let mut screen = vec![0u32; W * H];

    while window.is_open() && !window.is_key_down(Key::Escape) {
        for i in 0..16 {
            chip8.set_key(i, false);
        }
        for key in window.get_keys() {
            if let Some(idx) = map_key(key) {
                chip8.set_key(idx, true);
            }
        }
        for _ in 0..10 {
            chip8.step();
        }
        chip8.tick_timers();
        if let Some(beeper) = &mut beeper {
            beeper.set(chip8.state.sound > 0);
        }
        chip8.framebuffer(&mut screen);
        if let Err(e) = window.update_with_buffer(&screen, W, H) {
            eprintln!("error de vídeo: {e}");
            break;
        }
        file.checkpoint(&chip8.state);
    }
    if let Some(beeper) = &mut beeper {
        beeper.set(false);
    }
    file.checkpoint(&chip8.state);
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--pack") {
        do_pack(&args);
        return;
    }

    if let Some(path) = launcher::inner_path() {
        run_emulator(&path, args.iter().any(|a| a == "--reset"));
        return;
    }

    let exe = std::env::current_exe().expect("no puedo resolver la ruta del exe");
    if CapsuleFile::open(&exe).is_ok() {
        launcher::relaunch_if_needed();
    }

    println!(
        "capsule-chip8\n\nGenerar una cápsula:\n  {} --pack --rom roma.ch8 --out juego.exe\n\nEjecutar una cápsula:\n  juego.exe [--reset]\n",
        PathBuf::from("capsule-chip8").display()
    );
}
