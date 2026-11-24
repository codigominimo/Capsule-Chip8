//! Intérprete CHIP-8 con E/S de disco mapeada en 0x100..=0x113.

use crate::capsule::CapsuleFile;

pub const DISK_IO_BASE: u16 = 0x100;
pub const DISK_BUFFER: usize = 0x102;
pub const DISK_COMMAND: usize = 0x112;
pub const DISK_STATUS: usize = 0x113;

pub const CMD_READ: u8 = 1;
pub const CMD_WRITE: u8 = 2;
pub const CMD_SIZE: u8 = 3;
pub const CMD_GROW: u8 = 4;

const FONTS: [u8; 80] = [
    0xF0, 0x90, 0x90, 0x90, 0xF0, // 0
    0x20, 0x60, 0x20, 0x20, 0x70, // 1
    0xF0, 0x10, 0xF0, 0x80, 0xF0, // 2
    0xF0, 0x10, 0xF0, 0x10, 0xF0, // 3
    0x90, 0x90, 0xF0, 0x10, 0x10, // 4
    0xF0, 0x80, 0xF0, 0x10, 0xF0, // 5
    0xF0, 0x80, 0xF0, 0x90, 0xF0, // 6
    0xF0, 0x10, 0x20, 0x40, 0x40, // 7
    0xF0, 0x90, 0xF0, 0x90, 0xF0, // 8
    0xF0, 0x90, 0xF0, 0x10, 0xF0, // 9
    0xF0, 0x90, 0xF0, 0x90, 0x90, // A
    0xE0, 0x90, 0xE0, 0x90, 0xE0, // B
    0xF0, 0x80, 0x80, 0x80, 0xF0, // C
    0xE0, 0x90, 0x90, 0x90, 0xE0, // D
    0xF0, 0x80, 0xF0, 0x80, 0xF0, // E
    0xF0, 0x80, 0xF0, 0x80, 0x80, // F
];

/// Estado completo de la máquina: se serializa dentro de la cápsula.
#[derive(Clone)]
pub struct MachineState {
    pub v: [u8; 16],
    pub i: u16,
    pub pc: u16,
    pub sp: u8,
    pub stack: [u16; 16],
    pub delay: u8,
    pub sound: u8,
    pub ram: [u8; 4096],
    pub gfx: [bool; 64 * 32],
}

impl MachineState {
    pub fn new(rom: &[u8]) -> Self {
        let mut s = Self {
            v: [0; 16],
            i: 0,
            pc: 0x200,
            sp: 0,
            stack: [0; 16],
            delay: 0,
            sound: 0,
            ram: [0; 4096],
            gfx: [false; 64 * 32],
        };
        s.ram[0x050..0x050 + FONTS.len()].copy_from_slice(&FONTS);
        let n = rom.len().min(4096 - 0x200);
        s.ram[0x200..0x200 + n].copy_from_slice(&rom[..n]);
        s
    }
}

pub struct Chip8 {
    pub state: MachineState,
    pub keys: [bool; 16],
    waiting: Option<u8>,
    rng: u32,
    pub disk: CapsuleFile,
}

impl Chip8 {
    pub fn new(state: MachineState, disk: CapsuleFile) -> Self {
        Self {
            state,
            keys: [false; 16],
            waiting: None,
            rng: 0x9e3779b9,
            disk,
        }
    }

    pub fn set_key(&mut self, key: usize, down: bool) {
        if key >= 16 {
            return;
        }
        self.keys[key] = down;
        if down {
            if let Some(reg) = self.waiting {
                self.state.v[reg as usize] = key as u8;
                self.waiting = None;
            }
        }
    }

    pub fn tick_timers(&mut self) {
        if self.state.delay > 0 {
            self.state.delay -= 1;
        }
        if self.state.sound > 0 {
            self.state.sound -= 1;
        }
    }

    pub fn framebuffer(&self, out: &mut [u32]) {
        for (dst, on) in out.iter_mut().zip(self.state.gfx.iter()) {
            *dst = if *on { 0x00f0f0f0 } else { 0x00000000 };
        }
    }

    fn read_mem(&self, a: usize) -> u8 {
        if a == DISK_STATUS {
            return self.state.ram[DISK_STATUS];
        }
        self.state.ram[a]
    }

    pub fn write_mem(&mut self, a: usize, val: u8) {
        if a == DISK_COMMAND {
            self.disk_command(val);
        } else {
            self.state.ram[a] = val;
        }
    }

    fn disk_command(&mut self, cmd: u8) {
        let sector = u16::from_le_bytes([self.state.ram[0x100], self.state.ram[0x101]]) as usize;
        let status = match cmd {
            CMD_READ => match self.disk.read_sector(sector) {
                Some(data) => {
                    self.state.ram[DISK_BUFFER..DISK_BUFFER + 16].copy_from_slice(&data);
                    0
                }
                None => 0xff,
            },
            CMD_WRITE => {
                let mut data = [0u8; 16];
                data.copy_from_slice(&self.state.ram[DISK_BUFFER..DISK_BUFFER + 16]);
                match self.disk.write_sector(sector, &data, &self.state) {
                    true => 0,
                    false => 0xff,
                }
            }
            CMD_SIZE => {
                let sectors = (self.disk.disk_len() / 16) as u16;
                let b = sectors.to_le_bytes();
                self.state.ram[DISK_BUFFER] = b[0];
                self.state.ram[DISK_BUFFER + 1] = b[1];
                0
            }
            CMD_GROW => match self.disk.grow(&self.state) {
                true => 0,
                false => 0xff,
            },
            _ => 0xff,
        };
        self.state.ram[DISK_STATUS] = status;
    }

    pub fn step(&mut self) {
        if self.waiting.is_some() {
            return;
        }
        let pc = self.state.pc as usize;
        let op = ((self.state.ram[pc] as u16) << 8) | self.state.ram[pc + 1] as u16;
        let x = ((op >> 8) & 0x0f) as usize;
        let y = ((op >> 4) & 0x0f) as usize;
        let k = (op & 0x00ff) as u8;
        let nnn = (op & 0x0fff) as u16;
        let mut next = pc as u16 + 2;

        match op & 0xf000 {
            0x0000 => match op {
                0x00e0 => self.state.gfx.fill(false),
                0x00ee => {
                    self.state.sp = self.state.sp.saturating_sub(1);
                    next = self.state.stack[self.state.sp as usize];
                }
                _ => {}
            },
            0x1000 => next = nnn,
            0x2000 => {
                let sp = self.state.sp as usize;
                if sp < 16 {
                    self.state.stack[sp] = pc as u16 + 2;
                    self.state.sp = sp as u8 + 1;
                    next = nnn;
                }
            }
            0x3000 => {
                if self.state.v[x] == k {
                    next += 2;
                }
            }
            0x4000 => {
                if self.state.v[x] != k {
                    next += 2;
                }
            }
            0x5000 => {
                if self.state.v[x] == self.state.v[y] {
                    next += 2;
                }
            }
            0x6000 => self.state.v[x] = k,
            0x7000 => self.state.v[x] = self.state.v[x].wrapping_add(k),
            0x8000 => match op & 0x000f {
                0x0 => self.state.v[x] = self.state.v[y],
                0x1 => self.state.v[x] |= self.state.v[y],
                0x2 => self.state.v[x] &= self.state.v[y],
                0x3 => self.state.v[x] ^= self.state.v[y],
                0x4 => {
                    let (r, c) = self.state.v[x].overflowing_add(self.state.v[y]);
                    self.state.v[x] = r;
                    self.state.v[0xf] = c as u8;
                }
                0x5 => {
                    let (r, b) = self.state.v[x].overflowing_sub(self.state.v[y]);
                    self.state.v[x] = r;
                    self.state.v[0xf] = !b as u8;
                }
                0x6 => {
                    let v = self.state.v[x];
                    self.state.v[x] = v >> 1;
                    self.state.v[0xf] = v & 1;
                }
                0x7 => {
                    let v = self.state.v[x];
                    self.state.v[x] = self.state.v[y].wrapping_sub(v);
                    self.state.v[0xf] = (self.state.v[y] >= v) as u8;
                }
                0xe => {
                    let v = self.state.v[x];
                    self.state.v[x] = v << 1;
                    self.state.v[0xf] = v >> 7;
                }
                _ => {}
            },
            0x9000 => {
                if self.state.v[x] != self.state.v[y] {
                    next += 2;
                }
            }
            0xa000 => self.state.i = nnn,
            0xb000 => next = nnn.wrapping_add(self.state.v[0] as u16),
            0xc000 => {
                self.rng = self.rng.wrapping_mul(1664525).wrapping_add(1013904223);
                self.state.v[x] = (self.rng >> 16) as u8 & k;
            }
            0xd000 => {
                let sx = self.state.v[x] as usize % 64;
                let sy = self.state.v[y] as usize % 32;
                let rows = (op & 0x000f) as usize;
                let mut collision = false;
                for row in 0..rows {
                    let byte = self.state.ram[(self.state.i as usize + row) & 0xfff];
                    for bit in 0..8 {
                        if byte & (0x80 >> bit) != 0 {
                            let px = (sx + bit) % 64;
                            let py = (sy + row) % 32;
                            let idx = py * 64 + px;
                            if self.state.gfx[idx] {
                                collision = true;
                            }
                            self.state.gfx[idx] ^= true;
                        }
                    }
                }
                self.state.v[0xf] = collision as u8;
            }
            0xe000 => match k {
                0x9e => {
                    if self.keys[self.state.v[x] as usize & 0xf] {
                        next += 2;
                    }
                }
                0xa1 => {
                    if !self.keys[self.state.v[x] as usize & 0xf] {
                        next += 2;
                    }
                }
                _ => {}
            },
            0xf000 => match k {
                0x07 => self.state.v[x] = self.state.delay,
                0x0a => {
                    if !self.keys.iter().any(|&p| p) {
                        self.waiting = Some(x as u8);
                        return;
                    }
                    if let Some(key) = self.keys.iter().position(|&p| p) {
                        self.state.v[x] = key as u8;
                    }
                }
                0x15 => self.state.delay = self.state.v[x],
                0x18 => self.state.sound = self.state.v[x],
                0x1e => self.state.i = self.state.i.wrapping_add(self.state.v[x] as u16),
                0x29 => self.state.i = 0x050 + self.state.v[x] as u16 * 5,
                0x33 => {
                    let v = self.state.v[x];
                    let i = self.state.i as usize;
                    self.state.ram[i] = v / 100;
                    self.state.ram[i + 1] = (v / 10) % 10;
                    self.state.ram[i + 2] = v % 10;
                }
                0x55 => {
                    for j in 0..=x {
                        let val = self.state.v[j];
                        let at = self.state.i as usize + j;
                        self.write_mem(at, val);
                    }
                }
                0x65 => {
                    for j in 0..=x {
                        let at = self.state.i as usize + j;
                        self.state.v[j] = self.read_mem(at);
                    }
                }
                _ => {}
            },
            _ => {}
        }
        self.state.pc = next;
    }
}
