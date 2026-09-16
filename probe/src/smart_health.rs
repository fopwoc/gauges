use std::{
    collections::{BTreeSet, HashMap},
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use gauges_shared::{SmartDeviceHealthMetric, SmartHealthStatus, StorageMetric};
use tracing::{debug, warn};

use crate::storage::block;

const HEALTH_REFRESH_MS: i64 = 30 * 60 * 1_000;
const RETRY_REFRESH_MS: i64 = 5 * 60 * 1_000;
const COMMAND_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Default)]
pub(crate) struct SmartHealthCollector {
    cached: HashMap<String, SmartDeviceHealthMetric>,
}

impl SmartHealthCollector {
    pub(crate) fn collect(&mut self, storage: &mut [StorageMetric], sys_root: &Path, now_ms: i64) {
        let drives = storage
            .iter()
            .filter_map(|metric| match metric {
                StorageMetric::Btrfs { devices, .. } => Some(devices),
                _ => None,
            })
            .flatten()
            .filter_map(|device| block::whole_device_name(device, sys_root))
            .collect::<BTreeSet<_>>();

        if let Some(device) = drives.iter().find(|device| self.is_due(device, now_ms)) {
            let status = query_smart_status(device);
            if status == SmartHealthStatus::Failed {
                warn!(device, "SMART reports drive health failure");
            } else if status == SmartHealthStatus::Unavailable {
                debug!(device, "SMART health is unavailable");
            }
            self.cached.insert(
                device.clone(),
                SmartDeviceHealthMetric {
                    device: device.clone(),
                    status,
                    checked_at_ms: Some(now_ms),
                },
            );
        }

        for metric in storage {
            if let StorageMetric::Btrfs {
                devices,
                smart_health,
                ..
            } = metric
            {
                *smart_health = devices
                    .iter()
                    .filter_map(|device| block::whole_device_name(device, sys_root))
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .map(|device| {
                        self.cached
                            .get(&device)
                            .cloned()
                            .unwrap_or(SmartDeviceHealthMetric {
                                device,
                                status: SmartHealthStatus::Pending,
                                checked_at_ms: None,
                            })
                    })
                    .collect();
            }
        }
    }

    fn is_due(&self, device: &str, now_ms: i64) -> bool {
        self.cached.get(device).is_none_or(|health| {
            let interval = match health.status {
                SmartHealthStatus::Passed | SmartHealthStatus::Failed => HEALTH_REFRESH_MS,
                SmartHealthStatus::Pending
                | SmartHealthStatus::Standby
                | SmartHealthStatus::Unavailable => RETRY_REFRESH_MS,
            };
            health
                .checked_at_ms
                .is_none_or(|checked_at_ms| now_ms.saturating_sub(checked_at_ms) >= interval)
        })
    }
}

fn query_smart_status(device: &str) -> SmartHealthStatus {
    let Ok(mut child) = Command::new("smartctl")
        .args(["--json=c", "--health", "--nocheck=standby,3"])
        .arg(format!("/dev/{device}"))
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return SmartHealthStatus::Unavailable;
    };

    let deadline = Instant::now() + COMMAND_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return SmartHealthStatus::Unavailable;
            }
        }
    }
    let Ok(output) = child.wait_with_output() else {
        return SmartHealthStatus::Unavailable;
    };
    parse_smart_status(&output.stdout, output.status.code())
}

fn parse_smart_status(output: &[u8], exit_code: Option<i32>) -> SmartHealthStatus {
    let passed = serde_json::from_slice::<serde_json::Value>(output)
        .ok()
        .and_then(|json| {
            json.pointer("/smart_status/passed")
                .and_then(|value| value.as_bool())
        });
    match passed {
        Some(true) => SmartHealthStatus::Passed,
        Some(false) => SmartHealthStatus::Failed,
        None if exit_code == Some(3) => SmartHealthStatus::Standby,
        None if exit_code.is_some_and(|code| code & 8 != 0) => SmartHealthStatus::Failed,
        None => SmartHealthStatus::Unavailable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refreshes_success_less_often_than_unavailable_health() {
        let mut collector = SmartHealthCollector::default();
        collector.cached.insert(
            "sda".into(),
            SmartDeviceHealthMetric {
                device: "sda".into(),
                status: SmartHealthStatus::Passed,
                checked_at_ms: Some(1_000),
            },
        );
        collector.cached.insert(
            "sdb".into(),
            SmartDeviceHealthMetric {
                device: "sdb".into(),
                status: SmartHealthStatus::Unavailable,
                checked_at_ms: Some(1_000),
            },
        );
        assert!(!collector.is_due("sda", 1_000 + RETRY_REFRESH_MS));
        assert!(collector.is_due("sdb", 1_000 + RETRY_REFRESH_MS));
        assert!(collector.is_due("sda", 1_000 + HEALTH_REFRESH_MS));
    }

    #[test]
    fn parses_health_even_when_smartctl_reports_nonzero_exit() {
        assert_eq!(
            parse_smart_status(br#"{"smart_status":{"passed":true}}"#, Some(0)),
            SmartHealthStatus::Passed
        );
        assert_eq!(
            parse_smart_status(br#"{"smart_status":{"passed":false}}"#, Some(8)),
            SmartHealthStatus::Failed
        );
        assert_eq!(
            parse_smart_status(br#"{}"#, Some(3)),
            SmartHealthStatus::Standby
        );
        assert_eq!(
            parse_smart_status(br#"{}"#, Some(8)),
            SmartHealthStatus::Failed
        );
        assert_eq!(
            parse_smart_status(br#"{}"#, Some(2)),
            SmartHealthStatus::Unavailable
        );
    }
}
