use std::fmt;
use std::io::Read;
use std::time::Duration;

pub const TIMEOUT: Duration = Duration::from_secs(20);
pub const BODY_CAP_BYTES: u64 = 4 * 1024 * 1024;
pub const ATTEMPTS: u32 = 3;
pub const BACKOFF: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportError {
    Status(u16),
    Failed(String),
}

impl TransportError {
    pub fn transient(&self) -> bool {
        match self {
            Self::Status(status) => matches!(status, 408 | 425 | 429 | 500..=599),
            Self::Failed(_) => true,
        }
    }
}

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Status(status) => write!(f, "the site answered {status}"),
            Self::Failed(reason) => f.write_str(reason),
        }
    }
}

impl std::error::Error for TransportError {}

pub trait Transport: Send {
    fn get(&mut self, url: &str, accept: &str) -> Result<String, TransportError>;
}

pub struct UreqTransport {
    agent: ureq::Agent,
    cap: u64,
}

impl UreqTransport {
    pub fn new(user_agent: &str) -> Self {
        Self {
            agent: ureq::AgentBuilder::new()
                .user_agent(user_agent)
                .timeout(TIMEOUT)
                .build(),
            cap: BODY_CAP_BYTES,
        }
    }
}

impl Transport for UreqTransport {
    fn get(&mut self, url: &str, accept: &str) -> Result<String, TransportError> {
        let response =
            self.agent
                .get(url)
                .set("Accept", accept)
                .call()
                .map_err(|error| match error {
                    ureq::Error::Status(status, _) => TransportError::Status(status),
                    other => TransportError::Failed(format!("the fetch failed: {other}")),
                })?;
        let mut body = String::new();
        response
            .into_reader()
            .take(self.cap)
            .read_to_string(&mut body)
            .map_err(|error| {
                TransportError::Failed(format!("the page did not read as text: {error}"))
            })?;
        Ok(body)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GaveUp {
    pub error: TransportError,
    pub attempts: u32,
}

impl fmt::Display for GaveUp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.error.fmt(f)?;
        if self.attempts > 1 {
            write!(f, " (after {} attempts)", self.attempts)?;
        }
        Ok(())
    }
}

impl std::error::Error for GaveUp {}

#[derive(Debug, Clone, Copy)]
pub struct Retry {
    pub attempts: u32,
    pub backoff: Duration,
    pub sleep: fn(Duration),
}

impl Default for Retry {
    fn default() -> Self {
        Self {
            attempts: ATTEMPTS,
            backoff: BACKOFF,
            sleep: std::thread::sleep,
        }
    }
}

impl Retry {
    pub fn once() -> Self {
        Self::immediate(1)
    }

    pub fn immediate(attempts: u32) -> Self {
        Self {
            attempts,
            backoff: Duration::ZERO,
            sleep: |_| {},
        }
    }

    pub fn run<T>(&self, mut op: impl FnMut() -> Result<T, TransportError>) -> Result<T, GaveUp> {
        let attempts = self.attempts.max(1);
        let mut attempt = 0;
        loop {
            attempt += 1;
            match op() {
                Ok(value) => return Ok(value),
                Err(error) if error.transient() && attempt < attempts => {
                    let pause = self.backoff.saturating_mul(1 << (attempt - 1).min(16));
                    (self.sleep)(pause);
                }
                Err(error) => {
                    return Err(GaveUp {
                        error,
                        attempts: attempt,
                    })
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scripted(
        script: Vec<Result<&'static str, TransportError>>,
    ) -> (
        impl FnMut() -> Result<&'static str, TransportError>,
        std::rc::Rc<std::cell::Cell<u32>>,
    ) {
        let calls = std::rc::Rc::new(std::cell::Cell::new(0));
        let counter = calls.clone();
        let mut script = std::collections::VecDeque::from(script);
        let op = move || {
            counter.set(counter.get() + 1);
            script
                .pop_front()
                .unwrap_or(Err(TransportError::Failed("script exhausted".into())))
        };
        (op, calls)
    }

    #[test]
    fn transient_failures_are_retried_until_one_succeeds() {
        let (op, calls) = scripted(vec![
            Err(TransportError::Failed("timeout".into())),
            Err(TransportError::Status(503)),
            Ok("body"),
        ]);
        assert_eq!(Retry::immediate(3).run(op), Ok("body"));
        assert_eq!(calls.get(), 3);
    }

    #[test]
    fn exhausted_retries_report_the_last_error_and_the_count() {
        let (op, calls) = scripted(vec![
            Err(TransportError::Status(429)),
            Err(TransportError::Status(502)),
            Err(TransportError::Failed("connection reset".into())),
            Ok("never reached"),
        ]);
        let gave_up = Retry::immediate(3).run(op).unwrap_err();
        assert_eq!(gave_up.attempts, 3);
        assert_eq!(
            gave_up.error,
            TransportError::Failed("connection reset".into())
        );
        assert_eq!(gave_up.to_string(), "connection reset (after 3 attempts)");
        assert_eq!(calls.get(), 3);
    }

    #[test]
    fn client_errors_and_missing_pages_are_not_retried() {
        for status in [400, 403, 404] {
            let (op, calls) = scripted(vec![Err(TransportError::Status(status)), Ok("late")]);
            let gave_up = Retry::immediate(3).run(op).unwrap_err();
            assert_eq!(gave_up.error, TransportError::Status(status));
            assert_eq!(gave_up.attempts, 1);
            assert_eq!(gave_up.to_string(), format!("the site answered {status}"));
            assert_eq!(calls.get(), 1);
        }
    }

    #[test]
    fn a_single_attempt_policy_never_repeats() {
        let (op, calls) = scripted(vec![Err(TransportError::Status(500)), Ok("late")]);
        assert!(Retry::once().run(op).is_err());
        assert_eq!(calls.get(), 1);
        assert_eq!(Retry::immediate(0).attempts, 0);
        let (op, calls) = scripted(vec![Err(TransportError::Status(500))]);
        assert!(Retry::immediate(0).run(op).is_err());
        assert_eq!(calls.get(), 1);
    }
}
