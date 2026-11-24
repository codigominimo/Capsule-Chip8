//! Un mutex nominal por ruta para que dos cápsulas del mismo exe no se pisen.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;

fn path_hash(path: &Path) -> u64 {
    let mut h = DefaultHasher::new();
    path.hash(&mut h);
    h.finish()
}

pub struct InstanceLock {
    #[cfg(windows)]
    handle: *mut std::ffi::c_void,
    #[cfg(unix)]
    fd: i32,
}

impl InstanceLock {
    pub fn acquire(path: &Path) -> Result<Self, String> {
        let hash = path_hash(path);
        #[cfg(windows)]
        {
            let name: Vec<u16> = format!(r"Local\CapsuleCH8-{hash:016x}")
                .encode_utf16()
                .chain(Some(0))
                .collect();
            unsafe {
                #[link(name = "kernel32")]
                unsafe extern "system" {
                    fn CreateMutexW(attr: *const std::ffi::c_void, own: i32, name: *const u16) -> *mut std::ffi::c_void;
                    fn GetLastError() -> u32;
                    fn CloseHandle(h: *mut std::ffi::c_void) -> i32;
                }
                let h = CreateMutexW(std::ptr::null(), 1, name.as_ptr());
                if h.is_null() || GetLastError() == 183 {
                    if !h.is_null() {
                        CloseHandle(h);
                    }
                    return Err("ya hay otra instancia de esta cápsula en marcha".into());
                }
                return Ok(Self { handle: h });
            }
        }
        #[cfg(unix)]
        {
            let fd = unsafe { libc::socket(libc::AF_UNIX, libc::SOCK_DGRAM, 0) };
            if fd < 0 {
                return Err("no puedo crear el socket de bloqueo".into());
            }
            let mut addr: libc::sockaddr_un = unsafe { std::mem::zeroed() };
            addr.sun_family = libc::AF_UNIX as u16;
            let name = format!("capsule-ch8-{hash:016x}");
            let bytes = name.as_bytes();
            if bytes.len() > 106 {
                unsafe { libc::close(fd) };
                return Err("nombre de bloqueo demasiado largo".into());
            }
            addr.sun_path[0] = 0;
            let signed: &[i8] = unsafe {
                std::slice::from_raw_parts(bytes.as_ptr() as *const i8, bytes.len())
            };
            addr.sun_path[1..1 + bytes.len()].copy_from_slice(signed);
            let len = 2 + 1 + bytes.len();
            let r = unsafe {
                libc::bind(
                    fd,
                    &addr as *const libc::sockaddr_un as *const libc::sockaddr,
                    len as u32,
                )
            };
            if r != 0 {
                unsafe { libc::close(fd) };
                return Err("ya hay otra instancia de esta cápsula en marcha".into());
            }
            return Ok(Self { fd });
        }
    }
}

impl Drop for InstanceLock {
    fn drop(&mut self) {
        #[cfg(windows)]
        unsafe {
            #[link(name = "kernel32")]
            unsafe extern "system" {
                fn CloseHandle(h: *mut std::ffi::c_void) -> i32;
                fn ReleaseMutex(h: *mut std::ffi::c_void) -> i32;
            }
            ReleaseMutex(self.handle);
            CloseHandle(self.handle);
        }
        #[cfg(unix)]
        unsafe {
            libc::close(self.fd);
        }
    }
}
