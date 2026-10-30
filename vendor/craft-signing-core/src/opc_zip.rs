//! Re-encode OPC archive headers without changing uncompressed signed parts.
#![cfg(windows)]
use std::{
    fs::{File, OpenOptions},
    io,
    path::Path,
};
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

pub(super) fn normalize_container(source: &Path, output: &Path) -> Result<(), String> {
    if source == output || output.exists() {
        return Err("output must be new".into());
    }
    let input = File::open(source).map_err(|e| e.to_string())?;
    let mut archive = ZipArchive::new(input).map_err(|e| e.to_string())?;
    if archive.is_empty() || archive.len() > 65_535 {
        return Err("OPC ZIP32 entry count limit".into());
    }
    // Bounded expansion prevents a crafted compressed archive from exhausting the disk.
    const MAX_TOTAL_UNCOMPRESSED: u64 = 1024 * 1024 * 1024;
    let mut total_uncompressed = 0u64;
    let mut seen = std::collections::HashSet::new();
    for idx in 0..archive.len() {
        let part = archive.by_index(idx).map_err(|e| e.to_string())?;
        let name = part.name();
        if name.is_empty()
            || name.starts_with('/')
            || name.contains('\\')
            || name.split('/').any(|c| c == ".." || c == ".")
            || !seen.insert(name.to_owned())
        {
            return Err("invalid or repeated OPC part name".into());
        }
        if !matches!(
            part.compression(),
            CompressionMethod::Stored | CompressionMethod::Deflated
        ) || part.size() > u32::MAX as u64
        {
            return Err("unsupported ZIP32 part".into());
        }
        total_uncompressed = total_uncompressed
            .checked_add(part.size())
            .ok_or("OPC inflated size overflow")?;
        if total_uncompressed > MAX_TOTAL_UNCOMPRESSED {
            return Err("OPC uncompressed document exceeds 1 GiB safety limit".into());
        }
    }
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .map_err(|e| e.to_string())?;
    let mut writer = ZipWriter::new(file);
    for idx in 0..archive.len() {
        let mut part = archive.by_index(idx).map_err(|e| e.to_string())?;
        let opts = SimpleFileOptions::default()
            .compression_method(part.compression())
            .large_file(false);
        if part.is_dir() {
            writer
                .add_directory(part.name(), opts)
                .map_err(|e| e.to_string())?;
        } else {
            let name = part.name().to_owned();
            writer.start_file(name, opts).map_err(|e| e.to_string())?;
            let written = io::copy(&mut part, &mut writer).map_err(|e| e.to_string())?;
            if written != part.size() {
                return Err("OPC part byte count mismatch".into());
            }
        }
    }
    writer
        .finish()
        .map_err(|e| e.to_string())?
        .sync_all()
        .map_err(|e| e.to_string())?;
    Ok(())
}
