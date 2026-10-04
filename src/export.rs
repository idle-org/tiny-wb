//! CSV serialization and collision-safe file creation, separate from decoding.

use crate::{config::OutputConfig, protocol::Sample};
use chrono::Local;
use std::{
    fs::{self, OpenOptions},
    io::{self, BufWriter, Write},
    path::PathBuf,
};

/// Write sample values to any writer, optionally with an explicit units header.
///
/// The caller decides when/how to open and flush a file. Every write error is
/// propagated; decimal formatting uses Rust's locale-independent representation.
pub fn write_csv(
    mut writer: impl Write,
    samples: &[Sample],
    include_header: bool,
) -> io::Result<()> {
    if include_header {
        writeln!(writer, "frequency_MHz,level_dBm")?;
    }
    for sample in samples {
        writeln!(writer, "{},{}", sample.frequency_mhz, sample.level_dbm)?;
    }
    Ok(())
}

/// Save an already validated scan without overwriting any previous acquisition.
///
/// Names use local time plus microseconds and a collision counter. A file is
/// reported only after flushing; failed writes remove the incomplete file.
/// Readers may see the in-progress file, so only the returned path means complete.
pub fn save_csv(samples: &[Sample], settings: &OutputConfig) -> io::Result<PathBuf> {
    fs::create_dir_all(&settings.directory)?;
    let stamp = Local::now().format("%Y%m%d %H_%M_%S_%6f").to_string();
    for suffix in 0..100 {
        let path = settings
            .directory
            .join(format!("{}_{stamp}_{suffix}.csv", settings.filename_prefix));
        let file = match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        };
        let mut writer = BufWriter::new(file);
        let result = write_csv(&mut writer, samples, settings.include_header)
            .and_then(|()| writer.flush())
            .and_then(|()| writer.get_ref().sync_all());
        drop(writer);
        if let Err(error) = result {
            let _ = fs::remove_file(&path);
            return Err(error);
        }
        return Ok(path);
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not allocate a unique scan filename",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_has_explicit_units_only_when_requested() {
        let samples = [Sample {
            frequency_mhz: 240.5,
            level_dbm: -64.25,
        }];
        let mut bytes = Vec::new();
        write_csv(&mut bytes, &samples, false).unwrap();
        assert_eq!(bytes, b"240.5,-64.25\n");
        bytes.clear();
        write_csv(&mut bytes, &samples, true).unwrap();
        assert_eq!(bytes, b"frequency_MHz,level_dBm\n240.5,-64.25\n");
    }

    #[test]
    fn write_errors_are_propagated() {
        struct Broken;
        impl Write for Broken {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::ErrorKind::BrokenPipe.into())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        assert_eq!(
            write_csv(Broken, &[], true).unwrap_err().kind(),
            io::ErrorKind::BrokenPipe
        );
    }

    #[test]
    fn scans_do_not_overwrite_each_other() {
        let directory = std::env::temp_dir().join(format!("tiny-wb-export-{}", std::process::id()));
        let settings = OutputConfig {
            directory: directory.clone(),
            filename_prefix: "test".into(),
            include_header: false,
        };
        let samples = [Sample {
            frequency_mhz: 1.0,
            level_dbm: -1.0,
        }];
        let first = save_csv(&samples, &settings).unwrap();
        let second = save_csv(&samples, &settings).unwrap();
        assert_ne!(first, second);
        assert_eq!(fs::read_to_string(first).unwrap(), "1,-1\n");
        assert_eq!(fs::read_to_string(second).unwrap(), "1,-1\n");
        fs::remove_dir_all(directory).unwrap();
    }
}
