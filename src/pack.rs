//! Inyecta un ROM y una cápsula nueva en una copia de este propio executable.

use crate::binary::{self, pad_to_alignment};
use crate::capsule;
use std::path::Path;

pub fn pack(rom_path: &Path, template: &Path, out: &Path) -> Result<usize, String> {
    let rom = std::fs::read(rom_path).map_err(|e| format!("no puedo leer el ROM: {e}"))?;
    if rom.len() > 4096 - 0x200 {
        return Err("el ROM no cabe en 0x200..0xFFF".into());
    }
    let mut exe = std::fs::read(template).map_err(|e| format!("no puedo leer el exe: {e}"))?;

    let cap = binary::find_section(&exe, ".capsule")
        .ok_or_else(|| "el exe no tiene sección .capsule".to_string())?;
    let disk = binary::find_section(&exe, ".disk")
        .ok_or_else(|| "el exe no tiene sección .disk".to_string())?;
    let align = binary::file_alignment(&exe).unwrap_or(512);

    let (mut cap_blob, mut disk_blob) = capsule::initial_image(&rom, capsule::DISK_CHUNK);
    pad_to_alignment(&mut cap_blob, align);
    pad_to_alignment(&mut disk_blob, align);

    let cap_off = binary::append_blob(&mut exe, &cap_blob);
    let disk_off = binary::append_blob(&mut exe, &disk_blob);
    binary::set_section(&mut exe, &cap, cap_off, cap_blob.len(), cap_blob.len());
    binary::set_section(&mut exe, &disk, disk_off, disk_blob.len(), disk_blob.len());

    std::fs::write(out, &exe).map_err(|e| format!("no puedo escribir la salida: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(out, std::fs::Permissions::from_mode(0o755));
    }
    Ok(exe.len())
}
