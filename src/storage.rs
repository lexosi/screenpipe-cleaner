/// Disk-usage utilities for the screenpipe data directory.
///
/// The primary entry point is `directory_size_bytes`, which recursively walks
/// a directory tree and sums the sizes of all regular files.
use std::path::Path;

/// Recursively sum the size in bytes of every file under `dir`.
///
/// Symbolic links are not followed to avoid double-counting.
/// Returns 0 if the directory does not exist, so callers don't need to
/// special-case a fresh installation.
pub fn directory_size_bytes(dir: &Path) -> Result<u64, Box<dyn std::error::Error>> {
    if !dir.exists() {
        return Ok(0);
    }

    let mut total: u64 = 0;
    visit_dir(dir, &mut total)?;
    Ok(total)
}

/// Walk `dir` recursively, adding each file's size to `acc`.
fn visit_dir(dir: &Path, acc: &mut u64) -> Result<(), Box<dyn std::error::Error>> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let metadata = entry.metadata()?;

        if metadata.is_dir() {
            visit_dir(&entry.path(), acc)?;
        } else if metadata.is_file() {
            *acc += metadata.len();
        }
        // Skip symlinks intentionally.
    }
    Ok(())
}

/// Attempt to delete a file at `path`, ignoring "not found" errors.
///
/// Returns `Ok(true)` if the file was deleted, `Ok(false)` if it was already
/// absent, and `Err` for any other filesystem error.
pub fn delete_file(path: &Path) -> Result<bool, std::io::Error> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e),
    }
}
