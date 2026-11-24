//! Lectura y parcheo mínimo de cabeceras PE y ELF64.
//!
//! El emulador nunca usa estas secciones mapeadas en memoria: abre su propio
//! fichero y lee los bytes directamente. Aquí solo necesitamos localizar las
//! secciones `.capsule` y `.disk`, y saber crecer la segunda.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Pe,
    Elf,
}

#[derive(Debug, Clone)]
pub struct SectionInfo {
    pub name: String,
    pub raw_offset: usize,
    pub raw_size: usize,
    pub virt_size: usize,
    pub virt_addr: usize,
    pub header_offset: usize,
    pub format: Format,
}

pub fn detect(bytes: &[u8]) -> Option<Format> {
    if bytes.len() >= 0x40 && &bytes[..2] == b"MZ" {
        let off = read_u32(bytes, 0x3c)? as usize;
        if off + 4 <= bytes.len() && &bytes[off..off + 4] == b"PE\0\0" {
            return Some(Format::Pe);
        }
    }
    if bytes.len() >= 16 && &bytes[..4] == b"\x7fELF" {
        return Some(Format::Elf);
    }
    None
}

fn read_u16(b: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(o..o + 2)?.try_into().ok()?))
}
fn read_u32(b: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(o..o + 4)?.try_into().ok()?))
}
fn read_u64(b: &[u8], o: usize) -> Option<u64> {
    Some(u64::from_le_bytes(b.get(o..o + 8)?.try_into().ok()?))
}
fn write_u32(b: &mut [u8], o: usize, v: u32) {
    b[o..o + 4].copy_from_slice(&v.to_le_bytes());
}
fn write_u64(b: &mut [u8], o: usize, v: u64) {
    b[o..o + 8].copy_from_slice(&v.to_le_bytes());
}

pub fn align_up(v: usize, a: usize) -> usize {
    if a <= 1 {
        return v;
    }
    (v + a - 1) / a * a
}

fn pe_offset(b: &[u8]) -> Option<usize> {
    let off = read_u32(b, 0x3c)? as usize;
    if off + 24 <= b.len() && &b[off..off + 4] == b"PE\0\0" {
        Some(off)
    } else {
        None
    }
}

pub fn file_alignment(b: &[u8]) -> Option<usize> {
    match detect(b)? {
        Format::Pe => {
            let pe = pe_offset(b)?;
            let a = read_u32(b, pe + 24 + 36)? as usize;
            Some(a.max(512))
        }
        Format::Elf => Some(8),
    }
}

fn cstr(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&c| c == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

pub fn find_section(b: &[u8], wanted: &str) -> Option<SectionInfo> {
    match detect(b)? {
        Format::Pe => {
            let pe = pe_offset(b)?;
            let num = read_u16(b, pe + 6)? as usize;
            let opt_size = read_u16(b, pe + 20)? as usize;
            let table = pe + 24 + opt_size;
            for i in 0..num {
                let h = table + 40 * i;
                if h + 40 > b.len() {
                    break;
                }
                let name = cstr(&b[h..h + 8]);
                if name == wanted {
                    return Some(SectionInfo {
                        name,
                        virt_size: read_u32(b, h + 8)? as usize,
                        virt_addr: read_u32(b, h + 12)? as usize,
                        raw_size: read_u32(b, h + 16)? as usize,
                        raw_offset: read_u32(b, h + 20)? as usize,
                        header_offset: h,
                        format: Format::Pe,
                    });
                }
            }
            None
        }
        Format::Elf => {
            if b.len() < 64 {
                return None;
            }
            let shoff = read_u64(b, 40)? as usize;
            let entsize = read_u16(b, 58)? as usize;
            let num = read_u16(b, 60)? as usize;
            let strndx = read_u16(b, 62)? as usize;
            if entsize < 64 || strndx >= num {
                return None;
            }
            let strh = shoff + strndx * entsize;
            let stroff = read_u64(b, strh + 24)? as usize;
            let strsize = read_u64(b, strh + 32)? as usize;
            for i in 0..num {
                let h = shoff + i * entsize;
                if h + 64 > b.len() {
                    break;
                }
                let no = read_u32(b, h)? as usize;
                if stroff + no >= b.len() {
                    continue;
                }
                let name = cstr(&b[stroff + no..stroff + strsize.min(b.len() - stroff)]);
                if name == wanted {
                    return Some(SectionInfo {
                        name,
                        raw_offset: read_u64(b, h + 24)? as usize,
                        raw_size: read_u64(b, h + 32)? as usize,
                        virt_size: read_u64(b, h + 32)? as usize,
                        virt_addr: 0,
                        header_offset: h,
                        format: Format::Elf,
                    });
                }
            }
            None
        }
    }
}

/// Añade `blob` al final del fichero y devuelve su offset ya alineado.
pub fn append_blob(b: &mut Vec<u8>, blob: &[u8]) -> usize {
    let align = file_alignment(b).unwrap_or(512);
    let start = align_up(b.len(), align);
    b.resize(start, 0);
    b.extend_from_slice(blob);
    start
}

pub fn pad_to_alignment(b: &mut Vec<u8>, align: usize) {
    let n = align_up(b.len(), align) - b.len();
    b.resize(b.len() + n, 0);
}

/// Reposiciona y redimensiona una sección existente.
pub fn set_section(
    b: &mut Vec<u8>,
    info: &SectionInfo,
    raw_offset: usize,
    raw_size: usize,
    virt_size: usize,
) {
    match info.format {
        Format::Pe => {
            // VirtualSize NO se toca: debe seguir cuadrando con la VA de la
            // siguiente sección o Windows rechaza la imagen. SizeOfRawData
            // puede ser mayor o menor que VirtualSize sin problema.
            write_u32(b, info.header_offset + 16, raw_size as u32);
            write_u32(b, info.header_offset + 20, raw_offset as u32);
            let _ = virt_size;
            let pe = pe_offset(b).unwrap();
            write_u32(b, pe + 24 + 64, 0);
        }
        Format::Elf => {
            write_u64(b, info.header_offset + 24, raw_offset as u64);
            write_u64(b, info.header_offset + 32, raw_size as u64);
        }
    }
}

/// Crece la sección de disco: añade bytes al final del fichero y actualiza
/// únicamente los campos de esa sección (y `SizeOfImage` en PE).
pub fn grow_section(b: &mut Vec<u8>, name: &str, delta: usize) -> Option<usize> {
    let info = find_section(b, name)?;
    let align = file_alignment(b).unwrap_or(512);
    let extra = align_up(delta, align);
    let start = align_up(b.len(), align);
    b.resize(start + extra, 0);

    match info.format {
        Format::Pe => {
            let new_raw = info.raw_size + extra;
            write_u32(b, info.header_offset + 16, new_raw as u32);
            let pe = pe_offset(b)?;
            write_u32(b, pe + 24 + 64, 0);
        }
        Format::Elf => {
            write_u64(b, info.header_offset + 32, (info.raw_size + extra) as u64);
        }
    }
    Some(start)
}
