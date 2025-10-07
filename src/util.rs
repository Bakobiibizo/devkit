/// Shared utility helpers will live here.
pub mod fs {
    use std::fs;
    use std::io;
    use std::path::Path;

    /// Ensure a directory exists, creating it recursively if needed.
    pub fn ensure_dir(path: &Path) -> io::Result<()> {
        if !path.exists() {
            fs::create_dir_all(path)?;
        }
        Ok(())
    }
}
