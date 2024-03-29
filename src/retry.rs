use log::{error, info};
use std::num::NonZeroU32;
use std::result::Result as StdResult;
use std::{thread, time::Duration};

pub struct RetryPolicy {
    total: NonZeroU32,
    max_backoff: Duration,
}

impl RetryPolicy {
    pub fn new(total: NonZeroU32, max_backoff: Duration) -> Self {
        Self { total, max_backoff }
    }

    fn get_backoff_time(&self, retry_count: u32) -> Duration {
        let backoff_value = Duration::from_secs(2u64.pow(retry_count));
        if backoff_value > self.max_backoff {
            self.max_backoff
        } else {
            backoff_value
        }
    }

    pub fn retry<F>(&self, request: F) -> StdResult<ureq::Response, anyhow::Error>
    where
        F: Fn() -> StdResult<ureq::Response, ureq::Error>,
    {
        for retry_count in 0..self.total.get() {
            let response = match request() {
                Ok(response) => response,
                Err(error) => {
                    if retry_count < self.total.get() - 1 {
                        let sleep_time = self.get_backoff_time(retry_count);
                        info!(
                            "Retrying request after {} seconds due to error {}",
                            sleep_time.as_secs(),
                            error
                        );
                        thread::sleep(sleep_time);
                        continue;
                    }

                    error!(
                        "Failed request after {} retries, giving up due to error {}",
                        retry_count, error
                    );
                    return Err(anyhow::Error::new(error)
                        .context(format!("Failed request after {retry_count} retries")));
                }
            };

            return Ok(response);
        }
        panic!("Ended retry loop unexpectedly");
    }
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self::new(NonZeroU32::new(10).unwrap(), Duration::from_secs(120))
    }
}

#[cfg(test)]
mod test_retry {
    use super::*;

    #[test]
    fn test_no_retry() {
        let policy = RetryPolicy {
            total: NonZeroU32::new(1).unwrap(),
            max_backoff: Duration::from_secs(0),
        };

        let retry_counter = std::cell::RefCell::new(0);
        let result = policy
            .retry(|| {
                *retry_counter.borrow_mut() += 1;
                ureq::Response::new(200, "OK", "")
            })
            .unwrap();
        assert_eq!(result.status(), 200);
        assert_eq!(retry_counter.into_inner(), 1);
    }

    #[test]
    fn test_retry_once() {
        let policy = RetryPolicy {
            total: NonZeroU32::new(2).unwrap(),
            max_backoff: Duration::from_secs(0),
        };

        let retry_counter = std::cell::RefCell::new(0);
        let result = policy
            .retry(|| {
                let mut retry_counter_ref = retry_counter.borrow_mut();
                *retry_counter_ref += 1;
                if *retry_counter_ref == 1 {
                    Err(ureq::Error::Status(
                        500,
                        ureq::Response::new(500, "Internal Server Error", "")?,
                    ))
                } else {
                    ureq::Response::new(200, "OK", "")
                }
            })
            .unwrap();
        assert_eq!(result.status(), 200);
        assert_eq!(retry_counter.into_inner(), 2);
    }

    #[test]
    fn test_retry_fail() {
        let policy = RetryPolicy {
            total: NonZeroU32::new(2).unwrap(),
            max_backoff: Duration::from_secs(0),
        };

        let retry_counter = std::cell::RefCell::new(0);
        let err = policy
            .retry(|| {
                let mut retry_counter_ref = retry_counter.borrow_mut();
                *retry_counter_ref += 1;
                Err(ureq::Error::Status(
                    500,
                    ureq::Response::new(500, "Internal Server Error", "")?,
                ))
            })
            .unwrap_err();
        let err: ureq::Error = err.downcast().unwrap();
        let status = match err {
            ureq::Error::Status(status, _) => status,
            _ => panic!("Unexpected error invariant"),
        };
        assert_eq!(status, 500);
        assert_eq!(retry_counter.into_inner(), 2);
    }

    #[test]
    fn test_retry_reset_on_success() {
        let policy = RetryPolicy {
            total: NonZeroU32::new(2).unwrap(),
            max_backoff: Duration::from_secs(0),
        };

        let retry_counter = std::cell::RefCell::new(0);
        let result = policy
            .retry(|| {
                let mut retry_counter_ref = retry_counter.borrow_mut();
                *retry_counter_ref += 1;
                if *retry_counter_ref == 1 {
                    Err(ureq::Error::Status(
                        500,
                        ureq::Response::new(500, "Internal Server Error", "")?,
                    ))
                } else {
                    ureq::Response::new(200, "OK", "")
                }
            })
            .unwrap();
        assert_eq!(result.status(), 200);
        assert_eq!(retry_counter.into_inner(), 2);

        // Try a second request, expect it to use a new retry counter
        let retry_counter = std::cell::RefCell::new(0);
        let result = policy
            .retry(|| {
                let mut retry_counter_ref = retry_counter.borrow_mut();
                *retry_counter_ref += 1;
                if *retry_counter_ref == 1 {
                    Err(ureq::Error::Status(
                        500,
                        ureq::Response::new(500, "Internal Server Error", "")?,
                    ))
                } else {
                    ureq::Response::new(200, "OK", "")
                }
            })
            .unwrap();
        assert_eq!(result.status(), 200);
        assert_eq!(retry_counter.into_inner(), 2);
    }
}
