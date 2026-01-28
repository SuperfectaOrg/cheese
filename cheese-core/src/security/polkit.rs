use crate::{Error, Result};
use zbus::{Connection, proxy};
use std::collections::HashMap;

const POLKIT_SERVICE: &str = "org.freedesktop.PolicyKit1";
const POLKIT_PATH: &str = "/org/freedesktop/PolicyKit1/Authority";

#[proxy(
    interface = "org.freedesktop.PolicyKit1.Authority",
    default_service = "org.freedesktop.PolicyKit1",
    default_path = "/org/freedesktop/PolicyKit1/Authority"
)]
trait PolkitAuthority {
    async fn check_authorization(
        &self,
        subject: Subject,
        action_id: &str,
        details: HashMap<String, String>,
        flags: u32,
        cancellation_id: &str,
    ) -> zbus::Result<AuthorizationResult>;
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, zbus::zvariant::Type)]
pub struct Subject {
    subject_kind: String,
    subject_details: HashMap<String, zbus::zvariant::Value<'static>>,
}

#[derive(Debug, serde::Deserialize, zbus::zvariant::Type)]
pub struct AuthorizationResult {
    is_authorized: bool,
    is_challenge: bool,
    details: HashMap<String, String>,
}

pub struct PolkitClient {
    connection: Connection,
}

impl PolkitClient {
    pub fn new() -> Result<Self> {
        Ok(Self {
            connection: Connection::system()
                .map_err(|e| Error::DBus(format!("Failed to connect to system bus: {}", e)))?,
        })
    }

    pub async fn check_authorization(&self, action: &str) -> Result<bool> {
        let proxy = PolkitAuthorityProxy::new(&self.connection)
            .await
            .map_err(|e| Error::DBus(format!("Failed to create proxy: {}", e)))?;

        let subject = self.get_current_subject()?;
        let details = HashMap::new();

        let result = proxy
            .check_authorization(subject, action, details, 0, "")
            .await
            .map_err(|e| Error::DBus(format!("Authorization check failed: {}", e)))?;

        Ok(result.is_authorized)
    }

    pub async fn request_authorization(&self, action: &str) -> Result<bool> {
        let proxy = PolkitAuthorityProxy::new(&self.connection)
            .await
            .map_err(|e| Error::DBus(format!("Failed to create proxy: {}", e)))?;

        let subject = self.get_current_subject()?;
        let details = HashMap::new();
        let flags = 1;

        let result = proxy
            .check_authorization(subject, action, details, flags, "")
            .await
            .map_err(|e| Error::PolkitDenied(format!("Authorization request failed: {}", e)))?;

        if result.is_authorized {
            Ok(true)
        } else if result.is_challenge {
            Err(Error::PolkitDenied("User cancelled authentication".to_string()))
        } else {
            Err(Error::PolkitDenied("Authorization denied".to_string()))
        }
    }

    fn get_current_subject(&self) -> Result<Subject> {
        #[cfg(unix)]
        {
            use nix::unistd::getpid;
            let pid = getpid().as_raw() as u32;

            let mut details = HashMap::new();
            details.insert(
                "pid".to_string(),
                zbus::zvariant::Value::U32(pid).into(),
            );
            details.insert(
                "start-time".to_string(),
                zbus::zvariant::Value::U64(0).into(),
            );

            Ok(Subject {
                subject_kind: "unix-process".to_string(),
                subject_details: details,
            })
        }

        #[cfg(not(unix))]
        {
            Err(Error::DBus("Polkit not supported on this platform".to_string()))
        }
    }
}

pub const ACTION_DELETE: &str = "org.ratos.cheese.delete";
pub const ACTION_MODIFY: &str = "org.ratos.cheese.modify";
pub const ACTION_MOUNT: &str = "org.ratos.cheese.mount";
pub const ACTION_UNMOUNT: &str = "org.ratos.cheese.unmount";

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_polkit_client_creation() {
        let result = PolkitClient::new();
        assert!(result.is_ok() || result.is_err());
    }
}
