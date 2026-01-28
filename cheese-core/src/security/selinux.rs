use crate::{Error, Result};
use std::path::Path;
use std::ffi::CString;
use std::ptr;

pub fn is_enabled() -> bool {
    #[cfg(target_os = "linux")]
    {
        unsafe {
            let result = libc::access(
                b"/sys/fs/selinux\0".as_ptr() as *const libc::c_char,
                libc::F_OK,
            );
            result == 0
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        false
    }
}

pub fn check_context(path: &Path) -> Result<()> {
    if !is_enabled() {
        return Ok(());
    }

    #[cfg(target_os = "linux")]
    {
        let context = get_file_context(path)?;
        tracing::debug!("SELinux context for {}: {}", path.display(), context);
        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    {
        Ok(())
    }
}

#[cfg(target_os = "linux")]
pub fn get_file_context(path: &Path) -> Result<String> {
    let path_cstr = CString::new(path.to_string_lossy().as_bytes())
        .map_err(|_| Error::SelinuxContext("Invalid path".to_string()))?;

    unsafe {
        let mut context: *mut libc::c_char = ptr::null_mut();
        let result = lgetfilecon(
            path_cstr.as_ptr(),
            &mut context as *mut *mut libc::c_char,
        );

        if result < 0 {
            return Err(Error::SelinuxContext(format!(
                "Failed to get context for {}",
                path.display()
            )));
        }

        if context.is_null() {
            return Err(Error::SelinuxContext("Null context returned".to_string()));
        }

        let context_str = std::ffi::CStr::from_ptr(context)
            .to_string_lossy()
            .into_owned();

        freecon(context);

        Ok(context_str)
    }
}

#[cfg(target_os = "linux")]
pub fn set_file_context(path: &Path, context: &str) -> Result<()> {
    let path_cstr = CString::new(path.to_string_lossy().as_bytes())
        .map_err(|_| Error::SelinuxContext("Invalid path".to_string()))?;
    let context_cstr = CString::new(context)
        .map_err(|_| Error::SelinuxContext("Invalid context".to_string()))?;

    unsafe {
        let result = lsetfilecon(path_cstr.as_ptr(), context_cstr.as_ptr());

        if result < 0 {
            return Err(Error::SelinuxContext(format!(
                "Failed to set context for {}",
                path.display()
            )));
        }
    }

    Ok(())
}

#[cfg(target_os = "linux")]
pub fn restore_context(path: &Path) -> Result<()> {
    let path_cstr = CString::new(path.to_string_lossy().as_bytes())
        .map_err(|_| Error::SelinuxContext("Invalid path".to_string()))?;

    unsafe {
        let result = selinux_restorecon(path_cstr.as_ptr(), 0);

        if result < 0 {
            return Err(Error::SelinuxContext(format!(
                "Failed to restore context for {}",
                path.display()
            )));
        }
    }

    Ok(())
}

#[cfg(target_os = "linux")]
extern "C" {
    fn lgetfilecon(path: *const libc::c_char, con: *mut *mut libc::c_char) -> libc::c_int;
    fn lsetfilecon(path: *const libc::c_char, con: *const libc::c_char) -> libc::c_int;
    fn freecon(con: *mut libc::c_char);
    fn selinux_restorecon(pathname: *const libc::c_char, flags: libc::c_uint) -> libc::c_int;
}

#[cfg(not(target_os = "linux"))]
pub fn get_file_context(_path: &Path) -> Result<String> {
    Err(Error::SelinuxContext("SELinux not available".to_string()))
}

#[cfg(not(target_os = "linux"))]
pub fn set_file_context(_path: &Path, _context: &str) -> Result<()> {
    Err(Error::SelinuxContext("SELinux not available".to_string()))
}

#[cfg(not(target_os = "linux"))]
pub fn restore_context(_path: &Path) -> Result<()> {
    Err(Error::SelinuxContext("SELinux not available".to_string()))
}

pub fn validate_operation(path: &Path) -> Result<()> {
    if !is_enabled() {
        return Ok(());
    }

    check_context(path)?;

    let context = get_file_context(path)?;
    if context.contains("unlabeled") {
        tracing::warn!(
            "File has unlabeled SELinux context: {}",
            path.display()
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_selinux_enabled() {
        let enabled = is_enabled();
        println!("SELinux enabled: {}", enabled);
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_get_context() {
        if is_enabled() {
            let result = get_file_context(Path::new("/tmp"));
            println!("Context result: {:?}", result);
        }
    }
}
