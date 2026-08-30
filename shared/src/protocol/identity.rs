use serde::{Deserialize, Serialize};

use super::ValidationError;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProbeIdentity {
    pub hostname: String,
    pub distro: String,
    pub distro_version: String,
    pub kernel_version: String,
    pub ip_address: Option<String>,
    #[serde(default)]
    pub cpu_model: Option<String>,
    #[serde(default)]
    pub gpu_types: Vec<String>,
}

impl ProbeIdentity {
    pub(super) fn validate(&self) -> Result<(), ValidationError> {
        let values = [
            &self.hostname,
            &self.distro,
            &self.distro_version,
            &self.kernel_version,
        ];
        if values
            .into_iter()
            .any(|value| value.trim().is_empty() || value.len() > 512)
            || self
                .ip_address
                .as_ref()
                .is_some_and(|value| value.len() > 512)
            || self
                .cpu_model
                .as_ref()
                .is_some_and(|value| value.trim().is_empty() || value.len() > 512)
            || self.gpu_types.len() > 32
            || self
                .gpu_types
                .iter()
                .any(|value| value.trim().is_empty() || value.len() > 512)
        {
            return Err(ValidationError(
                "identity fields must contain 1 to 512 characters and gpuTypes at most 32 entries"
                    .into(),
            ));
        }
        Ok(())
    }
}
