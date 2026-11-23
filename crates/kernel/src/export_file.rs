//! Publish exports only after a complete, successful write. A cancelled worker
//! can drop its staged result without creating or replacing the user's file.
use std::{
    io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

pub struct StagedExport {
    temporary: PathBuf,
    destination: PathBuf,
}
impl StagedExport {
    pub fn new(destination: &Path) -> io::Result<Self> {
        if destination.as_os_str().is_empty() || destination.to_string_lossy().contains('\0') {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid export path",
            ));
        }
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let parent = destination
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        loop {
            let temporary = parent.join(format!(
                ".s33d-export-{}-{}.tmp",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)
            {
                Ok(_) => {
                    return Ok(Self {
                        temporary,
                        destination: destination.to_owned(),
                    })
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e),
            }
        }
    }
    pub fn path(&self) -> &Path {
        &self.temporary
    }
    pub fn commit(self) -> io::Result<()> {
        let file = std::fs::OpenOptions::new()
            .write(true)
            .open(&self.temporary)?;
        if file.metadata()?.len() == 0 {
            return Err(io::Error::other("Export produced an empty file"));
        }
        file.sync_all()?;
        drop(file);
        replace(&self.temporary, &self.destination)
    }
}
impl Drop for StagedExport {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.temporary);
    }
}
pub(crate) fn write(path: &str, bytes: &[u8]) -> io::Result<()> {
    let stage = StagedExport::new(Path::new(path))?;
    std::fs::write(stage.path(), bytes)?;
    stage.commit()
}

#[cfg(windows)]
fn replace(from: &Path, to: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    extern "system" {
        fn MoveFileExW(from: *const u16, to: *const u16, flags: u32) -> i32;
    }
    let from: Vec<_> = from.as_os_str().encode_wide().chain(Some(0)).collect();
    let to: Vec<_> = to.as_os_str().encode_wide().chain(Some(0)).collect();
    if unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), 1 | 8) } == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
#[cfg(not(windows))]
fn replace(from: &Path, to: &Path) -> io::Result<()> {
    std::fs::rename(from, to)
}
