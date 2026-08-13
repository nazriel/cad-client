//! The mechanics every wait loop shares: a total budget, a poll interval, and a
//! race against cancellation. What counts as finished stays with the domain.

use std::future::Future;
use std::time::Instant;

use crate::error::CliError;
use crate::exec::WaitOptions;

/// Polls until `is_finished` accepts a snapshot, the budget runs out, or
/// `cancel` completes.
///
/// `seed` is the snapshot the caller already holds — typically the response
/// that started the job — and is judged before anything is fetched. `None`
/// fetches first instead.
///
/// The terminal condition is a callback rather than a shared job abstraction on
/// purpose. A report is finished only once its file counters agree with its
/// status, duplication only on an explicit task status, and folding both into
/// one notion of "done" is exactly how the `Done`-with-zero-files case gets
/// lost. Returning `Err` is how a domain reports terminal *failure*, so remote
/// failures keep their own wording.
///
/// # Errors
///
/// Returns the error from `timeout_error` once the budget is spent,
/// [`CliError::Cancelled`] if `cancel` wins the race, and anything `fetch` or
/// `is_finished` returns.
pub(crate) async fn poll_until<T, Fetch, Fut, Finished, Cancel>(
    seed: Option<T>,
    options: WaitOptions,
    cancel: Cancel,
    mut fetch: Fetch,
    mut is_finished: Finished,
    timeout_error: impl FnOnce() -> CliError,
) -> Result<T, CliError>
where
    Fetch: FnMut() -> Fut,
    Fut: Future<Output = Result<T, CliError>>,
    Finished: FnMut(&T) -> Result<bool, CliError>,
    Cancel: Future<Output = ()>,
{
    tokio::pin!(cancel);
    let started = Instant::now();
    let mut current = match seed {
        Some(snapshot) => snapshot,
        None => fetch().await?,
    };

    loop {
        if is_finished(&current)? {
            return Ok(current);
        }
        if started.elapsed() >= options.timeout {
            return Err(timeout_error());
        }
        // The fetch deliberately sits outside the race: once the sleep wins,
        // the refresh runs to completion rather than being torn in half.
        tokio::select! {
            () = tokio::time::sleep(options.poll_interval) => {}
            () = &mut cancel => return Err(CliError::Cancelled),
        }
        current = fetch().await?;
    }
}
