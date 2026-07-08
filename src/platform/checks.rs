use std::path::Path;

fn contains_raspberry_pi(text: &str) -> bool {
    text.contains("Raspberry Pi")
}

/// Device-tree files are NUL-terminated byte strings.
fn model_is_raspberry_pi(bytes: &[u8]) -> bool {
    contains_raspberry_pi(String::from_utf8_lossy(bytes).trim_end_matches('\0'))
}

pub fn is_raspberry_pi() -> bool {
    // vOS kernels may not populate the /proc/cpuinfo "Model" line, so accept
    // either source.
    if std::fs::read_to_string("/proc/cpuinfo")
        .map(|v| contains_raspberry_pi(&v))
        .unwrap_or(false)
    {
        return true;
    }
    std::fs::read("/proc/device-tree/model")
        .map(|bytes| model_is_raspberry_pi(&bytes))
        .unwrap_or(false)
}

pub fn is_spi_enabled() -> bool {
    ["/dev/spidev0.0", "/dev/spidev0.1", "/dev/spidev1.0", "/dev/spidev1.1"]
        .iter()
        .any(|p| Path::new(p).exists())
}

/// True if the boot config contains an uncommented `dtparam=spi=on` line
/// (spaces around `=` allowed). Comment lines must not count.
fn config_enables_spi(content: &str) -> bool {
    content.lines().any(|line| {
        let line = line.trim();
        if line.starts_with('#') {
            return false;
        }
        let compact: String = line.chars().filter(|c| !c.is_whitespace()).collect();
        compact == "dtparam=spi=on" || compact.starts_with("dtparam=spi=on,")
    })
}

pub fn is_spi_enabled_config() -> bool {
    std::fs::read_to_string("/boot/firmware/config.txt")
        .map(|v| config_enables_spi(&v))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::{config_enables_spi, model_is_raspberry_pi};

    #[test]
    fn spi_config_commented_line_is_not_enabled() {
        assert!(!config_enables_spi("#dtparam=spi=on\n"));
        assert!(!config_enables_spi("  # dtparam=spi=on\n"));
    }

    #[test]
    fn spi_config_enabled_line_detected() {
        assert!(config_enables_spi("dtparam=spi=on\n"));
        assert!(config_enables_spi("  dtparam = spi = on \n"));
        assert!(config_enables_spi("dtparam=spi=on,audio=on\n"));
        assert!(config_enables_spi("#dtparam=spi=on\ndtparam=spi=on\n"));
    }

    #[test]
    fn spi_config_other_lines_are_not_enabled() {
        assert!(!config_enables_spi("dtparam=spi=off\n"));
        assert!(!config_enables_spi("dtparam=audio=on\n"));
        assert!(!config_enables_spi(""));
    }

    #[test]
    fn device_tree_model_nul_terminated() {
        assert!(model_is_raspberry_pi(b"Raspberry Pi 5 Model B Rev 1.0\0"));
        assert!(!model_is_raspberry_pi(b"Some Other Board\0"));
    }
}
