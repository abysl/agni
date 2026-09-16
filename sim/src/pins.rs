use crate::log::{LogAction, LogEntry};
use std::fmt;

pub fn hash_hex(hash: &[u8; 32]) -> String {
    let mut out = String::with_capacity(64);
    for byte in hash {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

const HEX: &[u8; 16] = b"0123456789abcdef";

pub fn blob_ref(hash: [u8; 32]) -> String {
    format!("blob:{}", hash_hex(&hash))
}

pub fn pin_hash(pin: &str) -> Option<[u8; 32]> {
    let hex = pin.strip_prefix("blob:")?;
    if hex.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(out)
}

pub fn genesis_engine_pin(log: &[LogEntry]) -> Option<String> {
    log.first().and_then(|entry| match &entry.action {
        LogAction::Genesis { config, .. } => config.engine.clone(),
        _ => None,
    })
}

pub fn genesis_plugin_pin(log: &[LogEntry]) -> Option<String> {
    log.first().and_then(|entry| match &entry.action {
        LogAction::Genesis { config, .. } => config.plugin.clone(),
        _ => None,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PinError {
    Mismatch { pinned: String, loaded: String },
    Missing { pinned: String },
    Unpinned { loaded: String },
}

impl PinError {
    fn describe(&self, kind: &str) -> String {
        match self {
            Self::Mismatch { pinned, loaded } => format!(
                "genesis pins {kind} {pinned} but this client loaded {loaded} — refusing to fold under a different {kind}"
            ),
            Self::Missing { pinned } => format!(
                "genesis pins {kind} {pinned} but this client has no {kind} module — install the pinned module to join"
            ),
            Self::Unpinned { loaded } => format!(
                "genesis pins no {kind} but this client loaded {loaded} — refusing to fold with an unpinned {kind}"
            ),
        }
    }
}

impl fmt::Display for PinError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.describe("module"))
    }
}

impl std::error::Error for PinError {}

fn verify(pinned: Option<String>, loaded: Option<&str>, unpinned_ok: bool) -> Result<(), PinError> {
    match (pinned, loaded) {
        (None, None) => Ok(()),
        (None, Some(loaded)) if unpinned_ok => {
            let _ = loaded;
            Ok(())
        }
        (None, Some(loaded)) => Err(PinError::Unpinned {
            loaded: loaded.into(),
        }),
        (Some(pinned), Some(loaded)) if pinned == loaded => Ok(()),
        (Some(pinned), Some(loaded)) => Err(PinError::Mismatch {
            pinned,
            loaded: loaded.into(),
        }),
        (Some(pinned), None) => Err(PinError::Missing { pinned }),
    }
}

pub fn verify_engine_pin(log: &[LogEntry], loaded: Option<&str>) -> Result<(), String> {
    verify(genesis_engine_pin(log), loaded, true).map_err(|error| error.describe("engine"))
}

pub fn verify_plugin_pin(log: &[LogEntry], loaded: Option<&str>) -> Result<(), String> {
    verify(genesis_plugin_pin(log), loaded, false).map_err(|error| error.describe("plugin"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pin_round_trips_through_its_hex() {
        let hash = [0xab; 32];
        let pin = blob_ref(hash);
        assert_eq!(pin.len(), 5 + 64);
        assert!(pin.starts_with("blob:abab"));
        assert_eq!(pin_hash(&pin), Some(hash));
        assert_eq!(pin_hash("blob:zz"), None);
        assert_eq!(pin_hash("nonsense"), None);
    }

    #[test]
    fn engine_pins_tolerate_an_unpinned_genesis_and_plugin_pins_do_not() {
        assert!(verify(None, Some("x"), true).is_ok());
        assert_eq!(
            verify(None, Some("x"), false),
            Err(PinError::Unpinned { loaded: "x".into() })
        );
        assert_eq!(
            verify(Some("a".into()), Some("b"), true),
            Err(PinError::Mismatch {
                pinned: "a".into(),
                loaded: "b".into()
            })
        );
        assert_eq!(
            verify(Some("a".into()), None, true),
            Err(PinError::Missing { pinned: "a".into() })
        );
        assert!(verify(Some("a".into()), Some("a"), false).is_ok());
    }
}
