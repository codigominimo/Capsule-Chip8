//! Intérprete CHIP-8 estándar, sin extensiones de E/S.

pub const SCREEN_W: usize = 64;
pub const SCREEN_H: usize = 32;
pub const SCALE: usize = 10;

const COLOR_ON: u32 = 0x00f0_f0f0;

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
    pub gfx: [bool; SCREEN_W * SCREEN_H],
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
            gfx: [false; SCREEN_W * SCREEN_H],
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
}

impl Chip8 {
    pub fn new(state: MachineState) -> Self {
        Self {
            state,
            keys: [false; 16],
            waiting: None,
            rng: 0x9e3779b9,
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

    /// Convierte la pantalla lógica de 64x32 en un buffer escalado 10x.
    pub fn framebuffer(&self, out: &mut [u32]) {
        let scaled_w = SCREEN_W * SCALE;
        let scaled_h = SCREEN_H * SCALE;
        assert!(
            out.len() >= scaled_w * scaled_h,
            "buffer de vídeo demasiado pequeño"
        );
        out.fill(0);
        for y in 0..SCREEN_H {
            for x in 0..SCREEN_W {
                if !self.state.gfx[y * SCREEN_W + x] {
                    continue;
                }
                for sy in 0..SCALE {
                    let row = (y * SCALE + sy) * scaled_w + x * SCALE;
                    out[row..row + SCALE].fill(COLOR_ON);
                }
            }
        }
    }

    fn read_mem(&self, a: usize) -> u8 {
        self.state.ram[a]
    }

    fn write_mem(&mut self, a: usize, val: u8) {
        self.state.ram[a] = val;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn framebuffer_scales_all_logical_pixels() {
        let mut state = MachineState::new(&[]);
        let w = SCREEN_W * SCALE;
        state.gfx[0] = true;
        state.gfx[SCREEN_W - 1] = true;
        state.gfx[(SCREEN_H - 1) * SCREEN_W] = true;

        let mut out = vec![0u32; w * SCREEN_H * SCALE];
        Chip8::new(state).framebuffer(&mut out);

        assert_eq!(out[0], COLOR_ON);
        assert_eq!(out[SCALE - 1], COLOR_ON);
        assert_eq!(out[w * SCALE], 0);

        let top_right = (SCREEN_W - 1) * SCALE;
        assert_eq!(out[top_right], COLOR_ON);
        assert_eq!(out[top_right + SCALE - 1], COLOR_ON);
        assert_eq!(out[top_right + w * SCALE], 0);

        let last_row = (SCREEN_H * SCALE - 1) * w;
        assert_eq!(out[last_row], COLOR_ON);
        assert_eq!(out[last_row + SCALE - 1], COLOR_ON);
        assert_eq!(out[last_row - w * SCALE], 0);

        assert_eq!(
            out.iter().filter(|&&c| c == COLOR_ON).count(),
            3 * SCALE * SCALE
        );
    }
}
