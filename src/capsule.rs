//! Formato de la cápsula: cabecera + ROM + dos snapshots del estado de la máquina.
//!
//! El CHIP-8 no tiene disco: lo único que se guarda es el estado de la máquina
//! para que el juego continúe donde lo dejó al relanzar el exe.

use crate::binary;
use crate::chip8::MachineState;
use crc32fast::Hasher;
use std::path::{Path, PathBuf};

pub const MAGIC: &[u8; 8] = b"CAPSULE1";
pub const VERSION: u32 = 2;
pub const HEADER_LEN: usize = 64;
pub const STATE_LEN: usize = 4411;

const H_VERSION: usize = 8;
const H_GENERATION: usize = 12;
const H_ROM_LEN: usize = 16;
const H_STATE_LEN: usize = 20;
const H_ACTIVE: usize = 24;
const H_CRC: usize = 32;

const S_PC: usize = 0;
const S_I: usize = 2;
const S_SP: usize = 4;
const S_DELAY: usize = 5;
const S_SOUND: usize = 6;
const S_V: usize = 7;
const S_STACK: usize = 23;
const S_RAM: usize = 55;
const S_GFX: usize = 4151;
const S_CRC: usize = 4407;

fn crc(buf: &[u8]) -> u32 {
    let mut h = Hasher::new();
    h.update(buf);
    h.finalize()
}

fn put_u32(b: &mut [u8], o: usize, v: u32) {
    b[o..o + 4].copy_from_slice(&v.to_le_bytes());
}
fn get_u32(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}
fn put_u16(b: &mut [u8], o: usize, v: u16) {
    b[o..o + 2].copy_from_slice(&v.to_le_bytes());
}
fn get_u16(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes(b[o..o + 2].try_into().unwrap())
}

pub fn encode_state(s: &MachineState) -> Vec<u8> {
    let mut b = vec![0u8; STATE_LEN];
    put_u16(&mut b, S_PC, s.pc);
    put_u16(&mut b, S_I, s.i);
    b[S_SP] = s.sp;
    b[S_DELAY] = s.delay;
    b[S_SOUND] = s.sound;
    b[S_V..S_V + 16].copy_from_slice(&s.v);
    for (j, v) in s.stack.iter().enumerate() {
        put_u16(&mut b, S_STACK + j * 2, *v);
    }
    b[S_RAM..S_RAM + 4096].copy_from_slice(&s.ram);
    for (j, on) in s.gfx.iter().enumerate() {
        if *on {
            b[S_GFX + j / 8] |= 1 << (j % 8);
        }
    }
    let check = crc(&b[..S_CRC]);
    put_u32(&mut b, S_CRC, check);
    b
}

pub fn decode_state(b: &[u8]) -> Option<MachineState> {
    if b.len() < STATE_LEN || get_u32(b, S_CRC) != crc(&b[..S_CRC]) {
        return None;
    }
    let mut s = MachineState {
        v: [0; 16],
        i: get_u16(b, S_I),
        pc: get_u16(b, S_PC),
        sp: b[S_SP],
        stack: [0; 16],
        delay: b[S_DELAY],
        sound: b[S_SOUND],
        ram: [0; 4096],
        gfx: [false; 64 * 32],
    };
    s.v.copy_from_slice(&b[S_V..S_V + 16]);
    for j in 0..16 {
        s.stack[j] = get_u16(b, S_STACK + j * 2);
    }
    s.ram.copy_from_slice(&b[S_RAM..S_RAM + 4096]);
    for j in 0..64 * 32 {
        s.gfx[j] = b[S_GFX + j / 8] & (1 << (j % 8)) != 0;
    }
    Some(s)
}

fn write_header(b: &mut [u8], rom_len: usize, generation: u32, active: u8) {
    b[..8].copy_from_slice(MAGIC);
    put_u32(b, H_VERSION, VERSION);
    put_u32(b, H_GENERATION, generation);
    put_u32(b, H_ROM_LEN, rom_len as u32);
    put_u32(b, H_STATE_LEN, STATE_LEN as u32);
    b[H_ACTIVE] = active;
    put_u32(b, H_CRC, crc(&b[..H_CRC]));
}

/// Construye el bloque que se inyecta en la sección `.capsule` del exe.
pub fn initial_image(rom: &[u8]) -> Vec<u8> {
    let state = MachineState::new(rom);
    let state_bytes = encode_state(&state);
    let mut cap = vec![0u8; HEADER_LEN];
    write_header(&mut cap, rom.len(), 1, 0);
    cap.extend_from_slice(rom);
    cap.extend_from_slice(&state_bytes);
    cap.extend_from_slice(&state_bytes);
    cap
}

pub struct CapsuleFile {
    bytes: Vec<u8>,
    path: PathBuf,
    cap_off: usize,
    rom_off: usize,
    state_a: usize,
    state_b: usize,
    rom_len: usize,
    generation: u32,
    active: u8,
}

impl CapsuleFile {
    pub fn open(path: &Path) -> Result<Self, String> {
        cleanup_stale_temps(path);
        let bytes = std::fs::read(path).map_err(|e| format!("no puedo leer {}: {e}", path.display()))?;
        let cap = binary::find_section(&bytes, ".capsule")
            .ok_or_else(|| "el exe no tiene sección .capsule".to_string())?;

        let cap_off = cap.raw_offset;
        let h = bytes
            .get(cap_off..cap_off + HEADER_LEN)
            .ok_or_else(|| "cabecera fuera del fichero".to_string())?;
        if &h[..8] != MAGIC {
            return Err("magic de cápsula inválido".into());
        }
        if get_u32(h, H_VERSION) != VERSION {
            return Err("versión de cápsula no soportada; regenera el exe".into());
        }
        if get_u32(h, H_CRC) != crc(&h[..H_CRC]) {
            return Err("CRC de cabecera inválido; usa --reset".into());
        }
        if get_u32(h, H_STATE_LEN) as usize != STATE_LEN {
            return Err("tamaño de estado incompatible".into());
        }

        let rom_len = get_u32(h, H_ROM_LEN) as usize;
        let generation = get_u32(h, H_GENERATION);
        let active = h[H_ACTIVE];
        if active > 1 {
            return Err("cabecera inconsistente".into());
        }

        let rom_off = cap_off + HEADER_LEN;
        let state_a = rom_off + rom_len;
        let state_b = state_a + STATE_LEN;
        if state_b + STATE_LEN > cap.raw_offset + cap.raw_size {
            return Err("snapshots fuera de la sección .capsule".into());
        }

        Ok(Self {
            bytes,
            path: path.to_path_buf(),
            cap_off,
            rom_off,
            state_a,
            state_b,
            rom_len,
            generation,
            active,
        })
    }

    pub fn rom(&self) -> &[u8] {
        &self.bytes[self.rom_off..self.rom_off + self.rom_len]
    }

    pub fn load_state(&self) -> MachineState {
        let first = if self.active == 0 { self.state_a } else { self.state_b };
        let second = if self.active == 0 { self.state_b } else { self.state_a };
        decode_state(&self.bytes[first..first + STATE_LEN])
            .or_else(|| decode_state(&self.bytes[second..second + STATE_LEN]))
            .unwrap_or_else(|| MachineState::new(self.rom()))
    }

    fn write_header(&mut self) {
        write_header(
            &mut self.bytes[self.cap_off..self.cap_off + HEADER_LEN],
            self.rom_len,
            self.generation,
            self.active,
        );
    }

    /// Guarda el snapshot de la máquina de forma atómica.
    pub fn checkpoint(&mut self, state: &MachineState) {
        let encoded = encode_state(state);
        let slot = if self.active == 0 { self.state_b } else { self.state_a };
        self.bytes[slot..slot + STATE_LEN].copy_from_slice(&encoded);
        self.active ^= 1;
        self.generation = self.generation.wrapping_add(1);
        self.write_header();
        self.commit_atomic();
    }

    fn commit_atomic(&mut self) -> bool {
        let tmp = temp_path(&self.path);
        if write_file(&tmp, &self.bytes).is_err() {
            return false;
        }
        let ok = replace(&tmp, &self.path).is_ok();
        if !ok {
            let _ = std::fs::remove_file(&tmp);
        }
        #[cfg(unix)]
        fsync_dir(self.path.parent().unwrap_or_else(|| Path::new(".")));
        ok
    }
}

fn cleanup_stale_temps(path: &Path) {
    let Some(dir) = path.parent() else { return };
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else { return };
    let prefix = format!(".{name}.capsule-tmp-");
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let n = entry.file_name();
        let s = n.to_string_lossy();
        let leftover_from_replace = s.starts_with("~RF") && s.ends_with(".TMP");
        if (s.starts_with(&prefix) && !s.ends_with(".exe")) || leftover_from_replace {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

fn temp_path(path: &Path) -> PathBuf {
    let name = format!(
        ".{}.capsule-tmp-{}",
        path.file_name().unwrap_or_default().to_string_lossy(),
        std::process::id()
    );
    path.with_file_name(name)
}

fn write_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut f = std::fs::File::create(path)?;
    f.write_all(bytes)?;
    f.sync_all()
}

#[cfg(unix)]
fn fsync_dir(path: &Path) {
    use std::os::unix::ffi::OsStrExt;
    if let Ok(c) = std::ffi::CString::new(path.as_os_str().as_bytes()) {
        unsafe {
            let fd = libc::open(c.as_ptr(), libc::O_RDONLY | libc::O_DIRECTORY);
            if fd >= 0 {
                libc::fsync(fd);
                libc::close(fd);
            }
        }
    }
}

#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn ReplaceFileW(
        replaced: *const u16,
        replacement: *const u16,
        backup: *const u16,
        flags: u32,
        exclude: *const std::ffi::c_void,
        reserved: *const std::ffi::c_void,
    ) -> i32;
}

#[cfg(windows)]
fn wide(p: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    p.as_os_str().encode_wide().chain(Some(0)).collect()
}

fn replace(src: &Path, dst: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        let a = wide(dst);
        let b = wide(src);
        let ok = unsafe {
            ReplaceFileW(
                a.as_ptr(),
                b.as_ptr(),
                std::ptr::null(),
                0,
                std::ptr::null(),
                std::ptr::null(),
            )
        };
        if ok != 0 {
            return Ok(());
        }
    }
    std::fs::rename(src, dst)
}
