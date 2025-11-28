use cached::proc_macro::cached;
use paperclip::actix::Apiv2Schema;
use serde::Serialize;
use std::fs;
use std::process::Command;

#[cfg(feature = "raspberry")]
mod raspberry;

pub fn start() {
    #[cfg(feature = "raspberry")]
    raspberry::start_raspberry_events_scanner();
}

// Generic platform info that works on any Linux system
#[derive(Debug, Clone, Serialize, Apiv2Schema)]
pub struct GenericPlatform {
    pub model: String,
    pub arch: String,
    pub cpu_name: String,
    pub kernel: String,
    pub os_name: String,
}

impl GenericPlatform {
    pub fn new() -> Self {
        Self {
            model: Self::get_model(),
            arch: Self::get_arch(),
            cpu_name: Self::get_cpu_name(),
            kernel: Self::get_kernel(),
            os_name: Self::get_os_name(),
        }
    }

    fn get_model() -> String {
        // Try device-tree first (ARM boards like Raspberry Pi, Jetson, Radxa)
        if let Ok(model) = fs::read_to_string("/proc/device-tree/model") {
            let model = model.trim().trim_matches(char::from(0)).to_string();
            if !model.is_empty() {
                return model;
            }
        }

        // Try DMI for x86 systems
        if let Ok(product) = fs::read_to_string("/sys/devices/virtual/dmi/id/product_name") {
            let product = product.trim().trim_matches(char::from(0)).to_string();
            if !product.is_empty() && product != "Default string" {
                return product;
            }
        }

        // Try board name
        if let Ok(board) = fs::read_to_string("/sys/devices/virtual/dmi/id/board_name") {
            let board = board.trim().trim_matches(char::from(0)).to_string();
            if !board.is_empty() && board != "Default string" {
                return board;
            }
        }

        "Unknown".to_string()
    }

    fn get_arch() -> String {
        String::from_utf8(
            Command::new("uname")
                .arg("-m")
                .output()
                .map(|o| o.stdout)
                .unwrap_or_default(),
        )
        .unwrap_or_else(|_| "Unknown".to_string())
        .trim()
        .to_string()
    }

    fn get_cpu_name() -> String {
        // Try lscpu first
        if let Ok(output) = Command::new("lscpu").output() {
            if let Ok(stdout) = String::from_utf8(output.stdout) {
                if let Some(line) = stdout.lines().find(|l| l.starts_with("Model name:")) {
                    if let Some(name) = line.split(':').nth(1) {
                        return name.trim().to_string();
                    }
                }
            }
        }

        // Fallback to /proc/cpuinfo
        if let Ok(cpuinfo) = fs::read_to_string("/proc/cpuinfo") {
            for line in cpuinfo.lines() {
                if line.starts_with("model name") || line.starts_with("Model") {
                    if let Some(name) = line.split(':').nth(1) {
                        return name.trim().to_string();
                    }
                }
            }
            // For ARM, try Hardware field
            for line in cpuinfo.lines() {
                if line.starts_with("Hardware") {
                    if let Some(name) = line.split(':').nth(1) {
                        return name.trim().to_string();
                    }
                }
            }
        }

        "Unknown".to_string()
    }

    fn get_kernel() -> String {
        String::from_utf8(
            Command::new("uname")
                .arg("-r")
                .output()
                .map(|o| o.stdout)
                .unwrap_or_default(),
        )
        .unwrap_or_else(|_| "Unknown".to_string())
        .trim()
        .to_string()
    }

    fn get_os_name() -> String {
        if let Ok(content) = fs::read_to_string("/etc/os-release") {
            for line in content.lines() {
                if line.starts_with("PRETTY_NAME=") {
                    return line
                        .trim_start_matches("PRETTY_NAME=")
                        .trim_matches('"')
                        .to_string();
                }
            }
        }
        "Linux".to_string()
    }
}

#[cfg(feature = "raspberry")]
#[derive(Debug, Clone, Serialize, Apiv2Schema)]
pub struct Raspberry {
    model: String,
    soc: String,
    serial: Option<String>,
    events: raspberry::Events,
}

#[cfg(feature = "raspberry")]
#[derive(Debug, Clone, Serialize, Apiv2Schema)]
pub struct Platform {
    raspberry: Option<Raspberry>,
    generic: GenericPlatform,
}

#[cfg(not(feature = "raspberry"))]
#[derive(Debug, Clone, Serialize, Apiv2Schema)]
pub struct Platform {
    pub generic: GenericPlatform,
}

#[cached(time = 5)]
pub fn platform() -> Result<Platform, String> {
    #[cfg(feature = "raspberry")]
    {
        use rppal;
        
        let raspberry_info = match rppal::system::DeviceInfo::new() {
            Ok(system) => Some(Raspberry {
                model: system.model().to_string(),
                soc: system.soc().to_string(),
                serial: get_raspberry_serial(),
                events: raspberry::events(),
            }),
            Err(_) => None,
        };

        return Ok(Platform {
            raspberry: raspberry_info,
            generic: GenericPlatform::new(),
        });
    }

    #[cfg(not(feature = "raspberry"))]
    Ok(Platform {
        generic: GenericPlatform::new(),
    })
}

#[cfg(feature = "raspberry")]
fn get_raspberry_serial() -> Option<String> {
    fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|content| {
            content
                .lines()
                .find(|line| line.starts_with("Serial"))
                .and_then(|line| line.split(':').nth(1))
                .map(|s| s.trim().to_string())
        })
}
