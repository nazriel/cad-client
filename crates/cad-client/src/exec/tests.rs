//! The shared wait mechanics. What counts as finished is tested in each
//! domain slice, beside the condition itself.

use std::time::Duration;

use super::WaitOptions;
use super::polling::poll_until;
use crate::error::CliError;

/// Covers the mechanics for both domains at once; the terminal conditions
/// themselves are tested where they are defined.
#[tokio::test]
async fn a_spent_budget_yields_the_domain_timeout() {
    let error = poll_until(
        Some(0_u8),
        WaitOptions {
            poll_interval: Duration::from_millis(1),
            timeout: Duration::ZERO,
        },
        std::future::pending(),
        || async { Ok(0_u8) },
        |_| Ok(false),
        || CliError::Timeout("job did not finish".to_owned()),
    )
    .await
    .expect_err("a zero budget must expire");

    assert!(matches!(error, CliError::Timeout(_)));
}

#[tokio::test]
async fn cancellation_wins_the_sleep() {
    let error = poll_until(
        Some(0_u8),
        WaitOptions {
            poll_interval: Duration::from_secs(30),
            timeout: Duration::from_secs(90),
        },
        std::future::ready(()),
        || async { Ok(0_u8) },
        |_| Ok(false),
        || CliError::Timeout("unreachable".to_owned()),
    )
    .await
    .expect_err("a completed cancellation must stop the wait");

    assert!(matches!(error, CliError::Cancelled));
}
