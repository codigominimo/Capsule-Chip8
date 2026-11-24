//! Arranca una copia del exe para que el archivo original quede libre y
//! pueda reescribirse y crecer mientras el emulador está en marcha.

use std::path::{Path, PathBuf};
use std::process::Command;

pub fn inner_path() -> Option<PathBuf> {
    let args: Vec<_> = std::env::args_os().collect();
    if args.len() >= 3 && args[1] == "--inner" {
        Some(PathBuf::from(&args[2]))
    } else {
        None
    }
}

fn temp_copy(original: &Path) -> PathBuf {
    let stem = original.file_stem().unwrap_or_default().to_string_lossy();
    if cfg!(windows) {
        original.with_file_name(format!(".{stem}.capsule-tmp-{}.exe", std::process::id()))
    } else {
        original.with_file_name(format!(".{stem}.capsule-tmp-{}", std::process::id()))
    }
}

pub fn relaunch_if_needed() {
    if inner_path().is_some() {
        return;
    }
    let original = std::env::current_exe().expect("no puedo resolver la ruta del exe");
    let extra: Vec<_> = std::env::args_os().skip(1).collect();

    #[cfg(target_os = "linux")]
    if try_memfd(&original, &extra) {
        unreachable!("execve sustituye al proceso");
    }

    let temp = temp_copy(&original);
    if let Err(e) = std::fs::copy(&original, &temp) {
        eprintln!("no puedo crear la copia temporal: {e}");
        std::process::exit(1);
    }
    let mut cmd = Command::new(&temp);
    cmd.arg("--inner").arg(&original);
    for arg in &extra {
        cmd.arg(arg);
    }
    let status = cmd
        .status()
        .unwrap_or_else(|e| {
            let _ = std::fs::remove_file(&temp);
            eprintln!("no puedo lanzar la copia: {e}");
            std::process::exit(1);
        });
    let _ = std::fs::remove_file(&temp);
    std::process::exit(status.code().unwrap_or(1));
}

#[cfg(target_os = "linux")]
fn try_memfd(original: &Path, extra: &[std::ffi::OsString]) -> bool {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let bytes = match std::fs::read(original) {
        Ok(b) => b,
        Err(_) => return false,
    };
    let name = CString::new("capsule-chip8").unwrap();
    unsafe {
        let fd = libc::syscall(libc::SYS_memfd_create, name.as_ptr(), 1u32) as i32;
        if fd < 0 {
            return false;
        }
        let mut off = 0usize;
        while off < bytes.len() {
            let n = libc::write(fd, bytes[off..].as_ptr() as *const libc::c_void, bytes.len() - off);
            if n <= 0 {
                libc::close(fd);
                return false;
            }
            off += n as usize;
        }
        if libc::fchmod(fd, 0o700) != 0 {
            libc::close(fd);
            return false;
        }

        let proc = CString::new(format!("/proc/self/fd/{fd}")).unwrap();
        let flag = CString::new("--inner").unwrap();
        let orig = match CString::new(original.as_os_str().as_bytes()) {
            Ok(v) => v,
            Err(_) => {
                libc::close(fd);
                return false;
            }
        };
        let mut extra_c: Vec<CString> = Vec::new();
        for a in extra {
            match CString::new(a.as_bytes()) {
                Ok(v) => extra_c.push(v),
                Err(_) => {
                    libc::close(fd);
                    return false;
                }
            }
        }
        let mut argv = vec![proc.as_ptr(), flag.as_ptr(), orig.as_ptr()];
        argv.extend(extra_c.iter().map(|s| s.as_ptr()));
        argv.push(std::ptr::null());
        let env: Vec<CString> = std::env::vars_os()
            .filter_map(|(k, v)| {
                let mut raw = k.as_bytes().to_vec();
                raw.push(b'=');
                raw.extend_from_slice(v.as_bytes());
                CString::new(raw).ok()
            })
            .collect();
        let mut envp: Vec<*const libc::c_char> = env.iter().map(|s| s.as_ptr()).collect();
        envp.push(std::ptr::null());

        libc::execve(proc.as_ptr(), argv.as_ptr(), envp.as_ptr());
        libc::close(fd);
        false
    }
}
