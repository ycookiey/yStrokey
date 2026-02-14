use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use ystrokey_core::{DiagnosticsConfig, DiagnosticsLevel};

struct FileLogger {
    config: DiagnosticsConfig,
    log_path: PathBuf,
}

static LOGGER: OnceLock<Mutex<FileLogger>> = OnceLock::new();

pub fn init(base_dir: &Path, config: &DiagnosticsConfig) {
    let log_path = base_dir.join("logs").join("ystrokey.log");
    if let Some(lock) = LOGGER.get() {
        if let Ok(mut logger) = lock.lock() {
            logger.log_path = log_path;
            logger.config = config.clone();
        }
        return;
    }

    let logger = FileLogger {
        config: config.clone(),
        log_path,
    };
    let _ = LOGGER.set(Mutex::new(logger));
}

pub fn update_config(config: &DiagnosticsConfig) {
    if let Some(lock) = LOGGER.get() {
        if let Ok(mut logger) = lock.lock() {
            logger.config = config.clone();
        }
    }
}

pub fn log(level: DiagnosticsLevel, message: &str) {
    let Some(lock) = LOGGER.get() else {
        eprintln!("[{}] {}", level_name(level), message);
        return;
    };

    let Ok(logger) = lock.lock() else {
        eprintln!("[{}] {}", level_name(level), message);
        return;
    };

    if !enabled(level, logger.config.level) {
        return;
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let line = format!("[{}][{}] {}\n", now, level_name(level), message);

    // Keep stderr output for visibility in development.
    eprint!("{}", line);

    if logger.config.file_logging_enabled {
        if let Err(err) = append_with_rotation(
            &logger.log_path,
            logger.config.max_file_bytes,
            logger.config.max_files,
            &line,
        ) {
            eprintln!("[ERROR] logger write failed: {}", err);
        }
    }
}

fn enabled(message_level: DiagnosticsLevel, configured_level: DiagnosticsLevel) -> bool {
    message_level <= configured_level
}

fn level_name(level: DiagnosticsLevel) -> &'static str {
    match level {
        DiagnosticsLevel::Error => "ERROR",
        DiagnosticsLevel::Warn => "WARN",
        DiagnosticsLevel::Info => "INFO",
        DiagnosticsLevel::Debug => "DEBUG",
    }
}

fn append_with_rotation(
    log_path: &Path,
    max_file_bytes: u64,
    max_files: u32,
    line: &str,
) -> std::io::Result<()> {
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let next_len = line.as_bytes().len() as u64;
    let current_len = fs::metadata(log_path).map(|m| m.len()).unwrap_or(0);

    if current_len.saturating_add(next_len) > max_file_bytes {
        rotate(log_path, max_files)?;
    }

    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)?;
    f.write_all(line.as_bytes())?;
    f.flush()?;
    Ok(())
}

fn rotate(log_path: &Path, max_files: u32) -> std::io::Result<()> {
    if max_files <= 1 {
        if log_path.exists() {
            fs::remove_file(log_path)?;
        }
        return Ok(());
    }

    for idx in (1..max_files).rev() {
        let src = if idx == 1 {
            log_path.to_path_buf()
        } else {
            rotated_path(log_path, idx - 1)
        };
        let dst = rotated_path(log_path, idx);

        if src.exists() {
            if dst.exists() {
                fs::remove_file(&dst)?;
            }
            fs::rename(&src, &dst)?;
        }
    }

    Ok(())
}

fn rotated_path(log_path: &Path, idx: u32) -> PathBuf {
    PathBuf::from(format!("{}.{}", log_path.to_string_lossy(), idx))
}
