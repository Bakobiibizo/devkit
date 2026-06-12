use std::{fs, io::Write};

use anyhow::{Context, Result};
use camino::Utf8Path;

pub(crate) fn prepend_release_section(path: &Utf8Path, section: &str) -> Result<()> {
    let mut content = if path.exists() {
        fs::read_to_string(path).with_context(|| format!("reading {}", path))?
    } else {
        String::from("# Changelog\n\n## Unreleased\n\n")
    };

    if let Some(index) = content.find("## Unreleased") {
        let insert_at = content[index..]
            .find('\n')
            .map(|offset| index + offset + 1)
            .unwrap_or(content.len());
        content.insert_str(insert_at, &format!("\n{}", section));
    } else {
        if !content.ends_with('\n') {
            content.push('\n');
        }
        content.push_str(section);
    }

    let mut file = fs::File::create(path).with_context(|| format!("opening {}", path))?;
    file.write_all(content.as_bytes())
        .with_context(|| format!("writing {}", path))?;
    Ok(())
}
