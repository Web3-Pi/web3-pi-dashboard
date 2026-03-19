use std::path::Path;

pub fn is_raspberry_pi() -> bool {
    std::fs::read_to_string("/proc/cpuinfo")
        .map(|v| v.contains("Raspberry Pi"))
        .unwrap_or(false)
}

pub fn is_spi_enabled() -> bool {
    ["/dev/spidev0.0", "/dev/spidev0.1", "/dev/spidev1.0", "/dev/spidev1.1"]
        .iter()
        .any(|p| Path::new(p).exists())
}

pub fn is_spi_enabled_config() -> bool {
    std::fs::read_to_string("/boot/firmware/config.txt")
        .map(|v| v.contains("dtparam=spi=on"))
        .unwrap_or(false)
}
