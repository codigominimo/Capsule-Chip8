#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use capsule_core::audio::Beeper;
use capsule_core::capsule::CapsuleFile;
use capsule_core::chip8::{self, Chip8, MachineState};
use capsule_core::keymap::{self, Keymap};
use capsule_core::{launcher, lock, pack};
use minifb::{Key, KeyRepeat, Window, WindowOptions};
use std::path::{Path, PathBuf};
use std::time::Instant;

// Espacio reservado para que el linker cree la sección. `--pack` la reubica al
// final de la copia y sustituye su contenido por la cápsula real.
#[used]
#[unsafe(link_section = ".capsule")]
static CAPSULE_RESERVED: [u8; 16384] = [0; 16384];

const W: usize = chip8::SCREEN_W * chip8::SCALE;
const H: usize = chip8::SCREEN_H * chip8::SCALE;

// El reloj del Chip8 no debe depender de los fps que logre minifb.
const CYCLES_PER_SECOND: f64 = 600.0;
const TIMER_TICKS_PER_SECOND: f64 = 60.0;
const MAX_FRAME_SECONDS: f64 = 0.25;

const UI_BG: u32 = 0x0000_0000;
const UI_ON: u32 = 0x00f0_f0f0;
const UI_DIM: u32 = 0x0070_7070;
const UI_SEL: u32 = 0x00f0_c050;
const UI_PANEL: u32 = 0x0020_2020;

const MENU_ITEMS: [&str; 4] = ["JUGAR", "CONTROLES", "REINICIAR", "SALIR"];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Screen {
    Menu,
    Config,
    Game,
}

const HEX: [&str; 16] = [
    "0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "A", "B", "C", "D", "E", "F",
];

// Orden físico del teclado CHIP-8: 1 2 3 C / 4 5 6 D / 7 8 9 E / A 0 B F.
const KEY_ORDER: [usize; 16] = [1, 2, 3, 12, 4, 5, 6, 13, 7, 8, 9, 14, 10, 0, 11, 15];

// Tipografía 3x5 para 0-9 y A-Z en posiciones fijas.
const GLYPHS: [[u8; 5]; 36] = [
    [7, 5, 5, 5, 7], [2, 6, 2, 2, 7], [7, 1, 7, 4, 7], [7, 1, 7, 1, 7],
    [5, 5, 7, 1, 1], [7, 4, 7, 1, 7], [7, 4, 7, 5, 7], [7, 1, 2, 2, 2],
    [7, 5, 7, 5, 7], [7, 5, 7, 1, 7], [2, 5, 7, 5, 5], [6, 5, 6, 5, 6],
    [7, 4, 4, 4, 7], [6, 5, 5, 5, 6], [7, 4, 6, 4, 7], [7, 4, 6, 4, 4],
    [7, 4, 5, 5, 7], [5, 5, 7, 5, 5], [7, 2, 2, 2, 7], [1, 1, 1, 5, 7],
    [5, 5, 6, 5, 5], [4, 4, 4, 4, 7], [5, 7, 7, 5, 5], [5, 7, 7, 7, 5],
    [7, 5, 5, 5, 7], [7, 5, 7, 4, 4], [7, 5, 5, 7, 1], [7, 5, 7, 6, 5],
    [7, 4, 7, 1, 7], [7, 2, 2, 2, 2], [5, 5, 5, 5, 7], [5, 5, 5, 5, 2],
    [5, 5, 7, 7, 5], [5, 5, 2, 5, 5], [5, 5, 2, 2, 2], [7, 1, 2, 4, 7],
];

/// `--pack` y la ayuda se ejecutan desde un terminal; el juego, no.
/// El binario es subsistema Windows, así que hay que recuperar la consola
/// del padre solo en esos casos.
#[cfg(target_os = "windows")]
fn ensure_console() {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn AttachConsole(process_id: u32) -> i32;
        fn AllocConsole() -> i32;
    }
    const ATTACH_PARENT_PROCESS: u32 = u32::MAX;
    unsafe {
        if AttachConsole(ATTACH_PARENT_PROCESS) == 0 {
            AllocConsole();
        }
    }
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

fn fill_rect(out: &mut [u32], x: usize, y: usize, w: usize, h: usize, color: u32) {
    for yy in y..(y + h).min(H) {
        let start = yy * W;
        for xx in x..(x + w).min(W) {
            out[start + xx] = color;
        }
    }
}

fn border_rect(out: &mut [u32], x: usize, y: usize, w: usize, h: usize, color: u32) {
    fill_rect(out, x, y, w, 1, color);
    fill_rect(out, x, y + h - 1, w, 1, color);
    fill_rect(out, x, y, 1, h, color);
    fill_rect(out, x + w - 1, y, 1, h, color);
}

fn glyph(ch: char) -> Option<&'static [u8]> {
    let idx = match ch {
        '0'..='9' => ch as usize - '0' as usize,
        'A'..='Z' => 10 + ch as usize - 'A' as usize,
        _ => return None,
    };
    GLYPHS.get(idx).map(|g| g.as_slice())
}

fn draw_text(out: &mut [u32], text: &str, x: usize, y: usize, scale: usize, color: u32) {
    let mut cx = x;
    for ch in text.chars() {
        if let Some(g) = glyph(ch) {
            for row in 0..5 {
                for col in 0..3 {
                    if g[row] & (1 << (2 - col)) != 0 {
                        fill_rect(
                            out,
                            cx + col * scale,
                            y + row * scale,
                            scale,
                            scale,
                            color,
                        );
                    }
                }
            }
        }
        cx += 4 * scale;
    }
}

fn draw_config(out: &mut [u32], keymap: &Keymap, selected: usize) {
    out.fill(UI_BG);
    draw_text(out, "CONTROLES", 40, 18, 4, UI_ON);

    let (x0, y0, cw, ch, gap) = (40usize, 70usize, 132usize, 48usize, 8usize);
    for (pos, &slot) in KEY_ORDER.iter().enumerate() {
        let x = x0 + (pos % 4) * (cw + gap);
        let y = y0 + (pos / 4) * (ch + gap);
        let is_selected = slot == selected;
        let color = if is_selected { UI_SEL } else { UI_ON };
        fill_rect(out, x, y, cw, ch, if is_selected { UI_PANEL } else { UI_BG });
        border_rect(out, x, y, cw, ch, if is_selected { UI_SEL } else { UI_DIM });
        draw_text(out, HEX[slot], x + 10, y + 14, 3, color);
        draw_text(out, keymap.label(slot), x + 64, y + 14, 3, color);
    }
    draw_text(out, "F1 SALIR IZQ DER ELEGIR ARR ABA CAMBIA", 40, 288, 2, UI_DIM);
}

fn draw_menu(out: &mut [u32], selected: usize) {
    out.fill(UI_BG);
    draw_text(out, "CAPSULE CHIP8", 40, 20, 5, UI_ON);
    draw_text(out, "MENU", 40, 70, 3, UI_DIM);

    for (i, label) in MENU_ITEMS.iter().enumerate() {
        let y = 108 + i * 44;
        let active = i == selected;
        let color = if active { UI_SEL } else { UI_ON };
        if active {
            fill_rect(out, 32, y - 6, 232, 38, UI_PANEL);
            border_rect(out, 32, y - 6, 232, 38, UI_SEL);
        }
        draw_text(out, label, 48, y, 3, color);
    }

    draw_text(
        out,
        "ARR ABA ELEGIR ENTER ACEPTAR F1 CONTROLES ESC SALIR",
        40,
        288,
        2,
        UI_DIM,
    );
}

/// Devuelve `true` cuando se debe salir de la configuración.
/// Los flancos de salida deben ser distintos del que abrió la pantalla.
fn handle_config(
    window: &Window,
    keymap: &mut Keymap,
    selected: &mut usize,
    file: &mut CapsuleFile,
    f1_exit: bool,
    enter_exit: bool,
) -> bool {
    if window.is_key_pressed(Key::Left, KeyRepeat::Yes) {
        *selected = (*selected + 15) % 16;
    }
    if window.is_key_pressed(Key::Right, KeyRepeat::Yes) {
        *selected = (*selected + 1) % 16;
    }
    if window.is_key_pressed(Key::Up, KeyRepeat::Yes) {
        keymap.cycle(*selected, -1);
        file.set_keymap(keymap);
    }
    if window.is_key_pressed(Key::Down, KeyRepeat::Yes) {
        keymap.cycle(*selected, 1);
        file.set_keymap(keymap);
    }
    for id in 0..keymap::HOST_KEY_COUNT as u8 {
        let key = keymap::key_from_id(id).expect("id de tecla válido");
        if matches!(
            key,
            Key::Left | Key::Right | Key::Up | Key::Down | Key::F1 | Key::Enter | Key::Escape
        ) {
            continue;
        }
        if window.is_key_pressed(key, KeyRepeat::No) {
            keymap.assign(*selected, id);
            file.set_keymap(keymap);
            *selected = (*selected + 1) % 16;
            break;
        }
    }
    f1_exit || enter_exit
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
    let mut keymap = file.load_keymap();
    let mut screen_state = Screen::Menu;
    let mut config_return = Screen::Menu;
    let mut menu_item = 0usize;
    let mut key_selected = 0usize;
    let mut config_just_opened = false;
    let mut f1_held = false;
    let mut enter_held = false;

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
    let mut last_frame = Instant::now();
    let mut cycle_acc = 0.0f64;
    let mut tick_acc = 0.0f64;

    while window.is_open() && !window.is_key_down(Key::Escape) {
        let now = Instant::now();
        let dt = now
            .duration_since(last_frame)
            .as_secs_f64()
            .min(MAX_FRAME_SECONDS);
        last_frame = now;

        let f1_down = window.is_key_down(Key::F1);
        let f1_pressed = f1_down && !f1_held;
        f1_held = f1_down;
        let enter_down = window.is_key_down(Key::Enter);
        let enter_pressed = enter_down && !enter_held;
        enter_held = enter_down;

        match screen_state {
            Screen::Menu => {
                if window.is_key_pressed(Key::Up, KeyRepeat::Yes) {
                    menu_item = (menu_item + MENU_ITEMS.len() - 1) % MENU_ITEMS.len();
                }
                if window.is_key_pressed(Key::Down, KeyRepeat::Yes) {
                    menu_item = (menu_item + 1) % MENU_ITEMS.len();
                }
                if f1_pressed {
                    screen_state = Screen::Config;
                    config_return = Screen::Menu;
                    config_just_opened = true;
                } else if enter_pressed {
                    match menu_item {
                        0 => screen_state = Screen::Game,
                        1 => {
                            screen_state = Screen::Config;
                            config_return = Screen::Menu;
                            config_just_opened = true;
                        }
                        2 => {
                            let fresh = MachineState::new(file.rom());
                            file.checkpoint(&fresh);
                            chip8 = Chip8::new(fresh);
                            screen_state = Screen::Game;
                        }
                        3 => break,
                        _ => unreachable!(),
                    }
                }
                if let Some(beeper) = &mut beeper {
                    beeper.set(false);
                }
                draw_menu(&mut screen, menu_item);
            }
            Screen::Config => {
                let f1_exit = f1_pressed && !config_just_opened;
                let enter_exit = enter_pressed && !config_just_opened;
                config_just_opened = false;
                if handle_config(
                    &window,
                    &mut keymap,
                    &mut key_selected,
                    &mut file,
                    f1_exit,
                    enter_exit,
                ) {
                    screen_state = config_return;
                }
                if let Some(beeper) = &mut beeper {
                    beeper.set(false);
                }
                draw_config(&mut screen, &keymap, key_selected);
            }
            Screen::Game => {
                if f1_pressed {
                    screen_state = Screen::Config;
                    config_return = Screen::Game;
                    config_just_opened = true;
                } else {
                    cycle_acc += dt * CYCLES_PER_SECOND;
                    tick_acc += dt * TIMER_TICKS_PER_SECOND;

                    for i in 0..16 {
                        chip8.set_key(i, false);
                    }
                    for i in 0..16 {
                        if let Some(key) = keymap.host_key(i) {
                            if window.is_key_down(key) {
                                chip8.set_key(i, true);
                            }
                        }
                    }

                    let cycles = cycle_acc.floor() as usize;
                    cycle_acc -= cycles as f64;
                    for _ in 0..cycles {
                        chip8.step();
                    }

                    // El tono se revisa en cada tick: si el sound timer llega a
                    // cero a mitad de un frame con pocos fps, el pitido se corta
                    // en ese instante.
                    if let Some(beeper) = &mut beeper {
                        beeper.set(chip8.state.sound > 0);
                    }
                    let ticks = tick_acc.floor() as usize;
                    tick_acc -= ticks as f64;
                    for _ in 0..ticks {
                        chip8.tick_timers();
                        if let Some(beeper) = &mut beeper {
                            beeper.set(chip8.state.sound > 0);
                        }
                    }

                    chip8.framebuffer(&mut screen);
                }
            }
        }

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
    let exe = std::env::current_exe().expect("no puedo resolver la ruta del exe");
    let is_capsule = CapsuleFile::open(&exe).is_ok();

    if args.iter().any(|a| a == "--pack") {
        #[cfg(target_os = "windows")]
        ensure_console();
        do_pack(&args);
        return;
    }

    if let Some(path) = launcher::inner_path() {
        run_emulator(&path, args.iter().any(|a| a == "--reset"));
        return;
    }

    if is_capsule {
        launcher::relaunch_if_needed();
        return;
    }

    #[cfg(target_os = "windows")]
    ensure_console();
    println!(
        "capsule-chip8\n\nGenerar una cápsula:\n  {} --pack --rom roms/Airplane.ch8 --out juego.exe\n\nEjecutar una cápsula:\n  juego.exe [--reset]  (menú al abrir; F1 configura los controles)\n",
        PathBuf::from("capsule-chip8").display()
    );
}
