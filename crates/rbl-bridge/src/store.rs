//! The snapshot kept in memory and rebuilt when rekordbox has written.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError, RwLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rbl_db::LibraryLocation;

use crate::snapshot::{Snapshot, Stamp};

/// The database files are looked at no more often than this.
const CHECK_EVERY: Duration = Duration::from_secs(1);
/// Looking for the rekordbox process scans every process: not on every request.
const RUNNING_EVERY: Duration = Duration::from_secs(5);

pub struct Store {
    location: LibraryLocation,
    current: RwLock<Arc<Snapshot>>,
    checked: Mutex<Instant>,
    rebuilding: tokio::sync::Mutex<()>,
    running: Mutex<Option<(Instant, bool)>>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Store {
    /// Reads the library once; an error here is a library that cannot be opened.
    pub fn open(location: LibraryLocation) -> Result<Self, String> {
        let snapshot = Snapshot::load(&location)?;
        Ok(Self {
            location,
            current: RwLock::new(Arc::new(snapshot)),
            checked: Mutex::new(Instant::now()),
            rebuilding: tokio::sync::Mutex::new(()),
            running: Mutex::new(None),
        })
    }

    fn current(&self) -> Arc<Snapshot> {
        Arc::clone(&self.current.read().unwrap_or_else(PoisonError::into_inner))
    }

    /// The snapshot, fresh to within a second of the database's last write.
    /// A rebuild that fails keeps the previous snapshot.
    pub async fn snapshot(&self) -> Arc<Snapshot> {
        let due = {
            let mut checked = lock(&self.checked);
            let due = checked.elapsed() >= CHECK_EVERY;
            if due {
                *checked = Instant::now();
            }
            due
        };
        if due && Stamp::of(&self.location.master_db) != self.current().stamp {
            self.rebuild().await;
        }
        self.current()
    }

    async fn rebuild(&self) {
        let _only_one = self.rebuilding.lock().await;
        if Stamp::of(&self.location.master_db) == self.current().stamp {
            return;
        }
        let location = self.location.clone();
        match tokio::task::spawn_blocking(move || Snapshot::load(&location)).await {
            Ok(Ok(fresh)) => *self.current.write().unwrap_or_else(PoisonError::into_inner) = Arc::new(fresh),
            Ok(Err(error)) => eprintln!("tempo-bridge: reading the library failed, keeping the previous snapshot: {error}"),
            Err(error) => eprintln!("tempo-bridge: reading the library stopped: {error}"),
        }
    }

    /// Whether rekordbox is running, looked up at most every few seconds.
    pub async fn rekordbox_running(&self) -> bool {
        if let Some((at, value)) = *lock(&self.running) {
            if at.elapsed() < RUNNING_EVERY {
                return value;
            }
        }
        let value = tokio::task::spawn_blocking(rbl_db::is_rekordbox_running).await.unwrap_or(false);
        *lock(&self.running) = Some((Instant::now(), value));
        value
    }
}

/// `2026-11-10T08:30:00Z` for a time, whole seconds.
pub fn iso8601(time: SystemTime) -> String {
    let secs = time.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let (days, rest) = (i64::try_from(secs / 86_400).unwrap_or(0), secs % 86_400);
    // Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z", rest / 3600, rest % 3600 / 60, rest % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_read_as_iso8601() {
        assert_eq!(iso8601(UNIX_EPOCH), "1970-01-01T00:00:00Z");
        assert_eq!(iso8601(UNIX_EPOCH + Duration::from_secs(1_782_000_000)), "2026-06-21T00:00:00Z");
        assert_eq!(iso8601(UNIX_EPOCH + Duration::from_secs(951_782_400 + 86_399)), "2000-02-29T23:59:59Z");
    }
}
