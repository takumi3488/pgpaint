use std::collections::HashMap;
use std::mem::size_of;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use crate::domain::connection_profile::ConnectionProfileId;
use crate::domain::database::{Cell, OwnerId, QueryId, QueryResult, StatementResult};
use crate::domain::query_run::{
    QueryRun, QueryRunOutcome, QueryRunStatus, QueryRunStore, QueryRunStoreError,
};

const MAX_RUNS: usize = 100;
const MAX_TOTAL_BYTES: usize = 64 * 1024 * 1024;
const MAX_RUN_BYTES: usize = 16 * 1024 * 1024;
const RUN_TTL: Duration = Duration::from_hours(1);
const MAX_ERROR_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct RunKey {
    owner: String,
    profile_id: uuid::Uuid,
    id: QueryId,
}

impl RunKey {
    fn new(owner: &OwnerId, profile_id: &ConnectionProfileId, id: QueryId) -> Self {
        Self {
            owner: owner.0.clone(),
            profile_id: profile_id.0,
            id,
        }
    }

    fn from_run(run: &QueryRun) -> Self {
        Self::new(&run.owner, &run.profile_id, run.id)
    }
}

struct StoredRun {
    run: Arc<QueryRun>,
    started_at: Instant,
    finished_at: Option<Instant>,
    bytes: usize,
}

#[derive(Default)]
struct StoreState {
    runs: HashMap<RunKey, StoredRun>,
    bytes: usize,
}

pub struct InMemoryQueryRunStore {
    state: Mutex<StoreState>,
    max_runs: usize,
    max_total_bytes: usize,
    max_run_bytes: usize,
    ttl: Duration,
}

impl InMemoryQueryRunStore {
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: Mutex::new(StoreState::default()),
            max_runs: MAX_RUNS,
            max_total_bytes: MAX_TOTAL_BYTES,
            max_run_bytes: MAX_RUN_BYTES,
            ttl: RUN_TTL,
        }
    }

    fn state(&self) -> MutexGuard<'_, StoreState> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn evict_expired(&self, state: &mut StoreState, now: Instant) {
        let expired = state
            .runs
            .iter()
            .filter(|(_, stored)| {
                stored
                    .finished_at
                    .is_some_and(|finished| now.duration_since(finished) >= self.ttl)
            })
            .map(|(key, _)| key.clone())
            .collect::<Vec<_>>();
        for key in expired {
            remove_run(state, &key);
        }
    }

    fn evict_oldest_finished(state: &mut StoreState, except: Option<&RunKey>) -> bool {
        let oldest = state
            .runs
            .iter()
            .filter(|(key, stored)| stored.finished_at.is_some() && except != Some(*key))
            .min_by_key(|(_, stored)| stored.finished_at)
            .map(|(key, _)| key.clone());
        oldest.is_some_and(|key| remove_run(state, &key))
    }
}

impl Default for InMemoryQueryRunStore {
    fn default() -> Self {
        Self::new()
    }
}

impl QueryRunStore for InMemoryQueryRunStore {
    fn insert(&self, run: QueryRun) -> Result<(), QueryRunStoreError> {
        let key = RunKey::from_run(&run);
        let bytes = run_size(&run);
        if bytes > self.max_run_bytes || bytes > self.max_total_bytes {
            return Err(QueryRunStoreError::RunTooLarge);
        }

        let now = Instant::now();
        let mut state = self.state();
        self.evict_expired(&mut state, now);
        if state.runs.contains_key(&key) {
            return Err(QueryRunStoreError::Full);
        }
        while state.runs.len() >= self.max_runs
            || state.bytes.saturating_add(bytes) > self.max_total_bytes
        {
            if !Self::evict_oldest_finished(&mut state, None) {
                return Err(QueryRunStoreError::Full);
            }
        }
        state.bytes += bytes;
        state.runs.insert(
            key,
            StoredRun {
                run: Arc::new(run),
                started_at: now,
                finished_at: None,
                bytes,
            },
        );
        Ok(())
    }

    fn get(
        &self,
        owner: &OwnerId,
        profile_id: &ConnectionProfileId,
        id: QueryId,
    ) -> Option<Arc<QueryRun>> {
        let now = Instant::now();
        let mut state = self.state();
        self.evict_expired(&mut state, now);
        state
            .runs
            .get(&RunKey::new(owner, profile_id, id))
            .map(|stored| Arc::clone(&stored.run))
    }

    fn request_cancel(
        &self,
        owner: &OwnerId,
        profile_id: &ConnectionProfileId,
        id: QueryId,
    ) -> bool {
        let now = Instant::now();
        let mut state = self.state();
        self.evict_expired(&mut state, now);
        state
            .runs
            .get_mut(&RunKey::new(owner, profile_id, id))
            .is_some_and(|stored| {
                if stored.run.status != QueryRunStatus::Running {
                    return false;
                }
                Arc::make_mut(&mut stored.run).cancel_requested = true;
                true
            })
    }

    fn clear_cancel_request(&self, owner: &OwnerId, profile_id: &ConnectionProfileId, id: QueryId) {
        if let Some(stored) = self
            .state()
            .runs
            .get_mut(&RunKey::new(owner, profile_id, id))
            && stored.run.status == QueryRunStatus::Running
        {
            Arc::make_mut(&mut stored.run).cancel_requested = false;
        }
    }

    fn finish(
        &self,
        owner: &OwnerId,
        profile_id: &ConnectionProfileId,
        id: QueryId,
        outcome: QueryRunOutcome,
    ) -> Option<Arc<QueryRun>> {
        let now = Instant::now();
        let key = RunKey::new(owner, profile_id, id);
        let mut state = self.state();
        self.evict_expired(&mut state, now);
        let old_bytes = state.runs.get(&key)?.bytes;
        let run_bytes_without_result = {
            let stored = state.runs.get(&key)?;
            run_size_without_result(&stored.run)
        };
        let available = self
            .max_run_bytes
            .saturating_sub(run_bytes_without_result)
            .min(
                self.max_total_bytes
                    .saturating_sub(state.bytes.saturating_sub(old_bytes))
                    .saturating_sub(run_bytes_without_result),
            );

        {
            let stored = state.runs.get_mut(&key)?;
            if stored.run.status != QueryRunStatus::Running {
                return Some(Arc::clone(&stored.run));
            }
            let started_at = stored.started_at;
            let run = Arc::make_mut(&mut stored.run);
            run.error = None;
            match outcome {
                QueryRunOutcome::Succeeded(mut result) => {
                    run.elapsed_ms = result.elapsed_ms;
                    trim_result(&mut result, available);
                    if result_size(&result) > available {
                        run.result = None;
                        run.error = Some(
                            "The query completed, but its result metadata exceeds the run-store limit.".to_owned(),
                        );
                    } else {
                        run.result = Some(result);
                    }
                    run.status = QueryRunStatus::Succeeded;
                }
                QueryRunOutcome::Failed(error) => {
                    run.elapsed_ms = elapsed_ms(started_at, now);
                    run.error = Some(limit_string(error, MAX_ERROR_BYTES));
                    run.result = None;
                    run.status = QueryRunStatus::Failed;
                }
                QueryRunOutcome::Canceled(error) => {
                    run.elapsed_ms = elapsed_ms(started_at, now);
                    run.error = error.map(|message| limit_string(message, MAX_ERROR_BYTES));
                    run.result = None;
                    run.status = QueryRunStatus::Canceled;
                }
            }
            let bytes = run_size(run);
            stored.finished_at = Some(now);
            stored.bytes = bytes;
        }

        let new_bytes = state.runs.get(&key)?.bytes;
        state.bytes = state
            .bytes
            .saturating_sub(old_bytes)
            .saturating_add(new_bytes);
        while state.bytes > self.max_total_bytes {
            if !Self::evict_oldest_finished(&mut state, Some(&key)) {
                break;
            }
        }
        state.runs.get(&key).map(|stored| Arc::clone(&stored.run))
    }
}

fn remove_run(state: &mut StoreState, key: &RunKey) -> bool {
    state.runs.remove(key).is_some_and(|stored| {
        state.bytes = state.bytes.saturating_sub(stored.bytes);
        true
    })
}

fn elapsed_ms(started_at: Instant, finished_at: Instant) -> u64 {
    u64::try_from(finished_at.duration_since(started_at).as_millis()).unwrap_or(u64::MAX)
}

fn limit_string(mut value: String, limit: usize) -> String {
    if value.len() > limit {
        let mut boundary = limit;
        while !value.is_char_boundary(boundary) {
            boundary -= 1;
        }
        value.truncate(boundary);
        value.push('…');
    }
    value
}

fn run_size(run: &QueryRun) -> usize {
    run_size_without_result(run).saturating_add(run.result.as_ref().map_or(0, result_size))
}

fn run_size_without_result(run: &QueryRun) -> usize {
    size_of::<QueryRun>()
        .saturating_add(run.sql.capacity())
        .saturating_add(
            run.warnings
                .capacity()
                .saturating_mul(size_of::<crate::domain::database::LintIssue>()),
        )
        .saturating_add(
            run.warnings
                .iter()
                .map(|issue| issue.message.capacity().saturating_add(issue.rule.len()))
                .sum::<usize>(),
        )
        .saturating_add(run.error.as_ref().map_or(0, String::capacity))
}

fn result_size(result: &QueryResult) -> usize {
    size_of::<QueryResult>()
        .saturating_add(
            result
                .statements
                .capacity()
                .saturating_mul(size_of::<StatementResult>()),
        )
        .saturating_add(result.statements.iter().map(statement_size).sum::<usize>())
}

fn statement_size(statement: &StatementResult) -> usize {
    size_of::<StatementResult>()
        .saturating_add(
            statement
                .columns
                .capacity()
                .saturating_mul(size_of::<crate::domain::database::ColumnMeta>()),
        )
        .saturating_add(
            statement
                .columns
                .iter()
                .map(|column| {
                    column
                        .name
                        .capacity()
                        .saturating_add(column.type_name.capacity())
                })
                .sum::<usize>(),
        )
        .saturating_add(
            statement
                .rows
                .capacity()
                .saturating_mul(size_of::<Vec<Cell>>()),
        )
        .saturating_add(statement.rows.iter().map(row_size).sum::<usize>())
}

fn row_size(row: &Vec<Cell>) -> usize {
    size_of::<Vec<Cell>>()
        .saturating_add(row.capacity().saturating_mul(size_of::<Cell>()))
        .saturating_add(
            row.iter()
                .map(|cell| cell.as_ref().map_or(0, String::capacity))
                .sum::<usize>(),
        )
}

fn trim_result(result: &mut QueryResult, budget: usize) {
    let mut fixed_bytes = size_of::<QueryResult>().saturating_add(
        result
            .statements
            .capacity()
            .saturating_mul(size_of::<StatementResult>()),
    );
    for statement in &mut result.statements {
        fixed_bytes = fixed_bytes
            .saturating_add(size_of::<StatementResult>())
            .saturating_add(
                statement
                    .columns
                    .capacity()
                    .saturating_mul(size_of::<crate::domain::database::ColumnMeta>()),
            )
            .saturating_add(
                statement
                    .columns
                    .iter()
                    .map(|column| {
                        column
                            .name
                            .capacity()
                            .saturating_add(column.type_name.capacity())
                    })
                    .sum::<usize>(),
            );
    }

    let mut remaining = budget.saturating_sub(fixed_bytes);
    for statement in &mut result.statements {
        // Rows are moved, not copied; rejected rows are released before the result is stored.
        let rows = std::mem::take(&mut statement.rows);
        let mut kept = Vec::new();
        for row in rows {
            let bytes = row_size(&row).saturating_add(size_of::<Vec<Cell>>());
            if bytes <= remaining {
                remaining -= bytes;
                kept.push(row);
            } else {
                statement.truncated = true;
            }
        }
        statement.rows = kept;
        statement.rows.shrink_to_fit();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::database::{ColumnMeta, StatementResult};

    fn run(owner: &str, profile_id: uuid::Uuid, id: uuid::Uuid) -> QueryRun {
        QueryRun {
            id: QueryId(id),
            owner: OwnerId(owner.to_owned()),
            profile_id: ConnectionProfileId(profile_id),
            sql: "SELECT 1".to_owned(),
            warnings: vec![],
            status: QueryRunStatus::Running,
            result: None,
            error: None,
            elapsed_ms: 0,
            cancel_requested: false,
        }
    }

    fn store(max_runs: usize, max_bytes: usize, max_run_bytes: usize) -> InMemoryQueryRunStore {
        InMemoryQueryRunStore {
            state: Mutex::new(StoreState::default()),
            max_runs,
            max_total_bytes: max_bytes,
            max_run_bytes,
            ttl: Duration::from_secs(60),
        }
    }

    #[test]
    fn run_lookup_is_scoped_to_owner_and_profile() {
        let store = store(10, MAX_TOTAL_BYTES, MAX_RUN_BYTES);
        let profile = uuid::Uuid::new_v4();
        let run = run("owner-a", profile, uuid::Uuid::new_v4());
        let id = run.id;
        assert!(store.insert(run).is_ok());

        assert!(
            store
                .get(
                    &OwnerId("owner-b".to_owned()),
                    &ConnectionProfileId(profile),
                    id
                )
                .is_none()
        );
        assert!(
            store
                .get(
                    &OwnerId("owner-a".to_owned()),
                    &ConnectionProfileId(uuid::Uuid::new_v4()),
                    id
                )
                .is_none()
        );
        assert!(
            store
                .get(
                    &OwnerId("owner-a".to_owned()),
                    &ConnectionProfileId(profile),
                    id
                )
                .is_some()
        );
    }

    #[test]
    fn result_rows_are_truncated_to_the_configured_memory_budget() {
        let store = store(10, 4096, 2048);
        let profile = uuid::Uuid::new_v4();
        let run = run("owner", profile, uuid::Uuid::new_v4());
        let id = run.id;
        let owner = run.owner.clone();
        let profile_id = run.profile_id.clone();
        assert!(store.insert(run).is_ok());
        let rows = (0..20)
            .map(|_| vec![Some("x".repeat(256))])
            .collect::<Vec<_>>();
        let result = QueryResult {
            statements: vec![StatementResult {
                columns: vec![ColumnMeta {
                    name: "value".to_owned(),
                    type_oid: 25,
                    type_name: "text".to_owned(),
                }],
                rows,
                rows_affected: 0,
                truncated: false,
            }],
            elapsed_ms: 5,
        };

        let final_run = store.finish(&owner, &profile_id, id, QueryRunOutcome::Succeeded(result));
        assert!(final_run.is_some());
        let statement = final_run
            .as_ref()
            .and_then(|run| run.result.as_ref())
            .and_then(|result| result.statements.first());
        assert!(statement.is_some());
        let Some(statement) = statement else {
            return;
        };
        assert!(statement.truncated);
        assert!(statement.rows.len() < 20);
    }

    #[test]
    fn capacity_evicts_oldest_terminal_runs_but_keeps_active_runs() {
        let store = store(1, MAX_TOTAL_BYTES, MAX_RUN_BYTES);
        let profile = uuid::Uuid::new_v4();
        let first = run("owner", profile, uuid::Uuid::new_v4());
        let first_id = first.id;
        let owner = first.owner.clone();
        let profile_id = first.profile_id.clone();
        assert!(store.insert(first).is_ok());
        assert!(
            store
                .finish(
                    &owner,
                    &profile_id,
                    first_id,
                    QueryRunOutcome::Failed("error".to_owned()),
                )
                .is_some()
        );

        let second = run("owner", profile, uuid::Uuid::new_v4());
        let second_id = second.id;
        assert!(store.insert(second).is_ok());
        assert!(store.get(&owner, &profile_id, first_id).is_none());
        assert!(store.get(&owner, &profile_id, second_id).is_some());
    }
}
