use std::{
    cmp::Reverse,
    collections::{BTreeSet, BinaryHeap, VecDeque},
};

use super::{REMOTE_ENRICHMENT_CONCURRENCY, REMOTE_ENRICHMENT_QUEUE_CAPACITY};

const SIMULATED_GROUP_COUNT: usize = 120;
const TMDB_REQUEST_MIN_INTERVAL_MS: u64 = 25;

#[derive(Debug, Clone, Copy)]
enum RemoteStage {
    Tmdb(u64),
    Artwork(u64),
    Database(u64),
}

impl RemoteStage {
    fn duration_ms(self) -> u64 {
        match self {
            Self::Tmdb(duration_ms) | Self::Artwork(duration_ms) | Self::Database(duration_ms) => {
                duration_ms
            }
        }
    }
}

#[derive(Debug, Clone)]
struct SimulatedGroup {
    local_analysis_ms: u64,
    remote_stages: Vec<RemoteStage>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SimulationResult {
    remote_concurrency: usize,
    queue_capacity: usize,
    group_count: usize,
    tmdb_request_count: usize,
    artwork_request_count: usize,
    local_compute_ms: u64,
    local_backpressure_ms: u64,
    local_pipeline_ms: u64,
    remote_pipeline_ms: u64,
    pipeline_wall_ms: u64,
    group_latency_p50_ms: u64,
    group_latency_p95_ms: u64,
    max_remote_in_flight: usize,
}

#[derive(Debug, Default)]
struct RemoteSimulationClock {
    next_tmdb_start_ms: u64,
    next_database_start_ms: u64,
}

fn mixed_first_scan_workload() -> Vec<SimulatedGroup> {
    (0..SIMULATED_GROUP_COUNT)
        .map(|index| {
            if index % 5 == 4 {
                let mut remote_stages = vec![
                    RemoteStage::Tmdb(180),
                    RemoteStage::Tmdb(160),
                    // The first local season. Remote-only seasons are not part
                    // of the workload after the local-season-only change.
                    RemoteStage::Tmdb(170),
                ];
                if index % 10 == 9 {
                    // Half of the simulated series have a second local season.
                    remote_stages.push(RemoteStage::Tmdb(170));
                }
                remote_stages.extend([
                    RemoteStage::Artwork(120),
                    RemoteStage::Artwork(140),
                    RemoteStage::Artwork(110),
                    RemoteStage::Artwork(90),
                    RemoteStage::Database(30),
                ]);
                SimulatedGroup {
                    local_analysis_ms: 140,
                    remote_stages,
                }
            } else {
                SimulatedGroup {
                    local_analysis_ms: 90,
                    remote_stages: vec![
                        RemoteStage::Tmdb(180),
                        RemoteStage::Tmdb(160),
                        RemoteStage::Artwork(120),
                        RemoteStage::Artwork(140),
                        RemoteStage::Database(25),
                    ],
                }
            }
        })
        .collect()
}

fn schedule_remote_stage(
    groups: &[SimulatedGroup],
    group_index: usize,
    worker_index: usize,
    stage_index: usize,
    now_ms: u64,
    clock: &mut RemoteSimulationClock,
    remote_events: &mut BinaryHeap<Reverse<(u64, usize, usize, usize)>>,
) {
    let stage = groups[group_index].remote_stages[stage_index];
    let started_at_ms = match stage {
        RemoteStage::Tmdb(_) => {
            let started_at_ms = now_ms.max(clock.next_tmdb_start_ms);
            clock.next_tmdb_start_ms = started_at_ms.saturating_add(TMDB_REQUEST_MIN_INTERVAL_MS);
            started_at_ms
        }
        RemoteStage::Database(_) => {
            let started_at_ms = now_ms.max(clock.next_database_start_ms);
            clock.next_database_start_ms = started_at_ms.saturating_add(stage.duration_ms());
            started_at_ms
        }
        RemoteStage::Artwork(_) => now_ms,
    };
    remote_events.push(Reverse((
        started_at_ms.saturating_add(stage.duration_ms()),
        worker_index,
        group_index,
        stage_index + 1,
    )));
}

fn simulate_remote_pipeline(
    groups: &[SimulatedGroup],
    remote_concurrency: usize,
    queue_capacity: usize,
) -> SimulationResult {
    assert!(remote_concurrency > 0);
    assert!(queue_capacity > 0);
    assert!(!groups.is_empty());

    let group_count = groups.len();
    let local_compute_ms = groups
        .iter()
        .map(|group| group.local_analysis_ms)
        .sum::<u64>();
    let tmdb_request_count = groups
        .iter()
        .flat_map(|group| group.remote_stages.iter())
        .filter(|stage| matches!(stage, RemoteStage::Tmdb(_)))
        .count();
    let artwork_request_count = groups
        .iter()
        .flat_map(|group| group.remote_stages.iter())
        .filter(|stage| matches!(stage, RemoteStage::Artwork(_)))
        .count();

    let mut now_ms = 0_u64;
    let mut local_completion = Some((groups[0].local_analysis_ms, 0_usize));
    let mut blocked_local_group = None::<(usize, u64)>;
    let mut local_ready_at = vec![None::<u64>; group_count];
    let mut local_pipeline_ms = 0_u64;

    let mut ready_groups = VecDeque::<usize>::new();
    let mut free_workers = (0..remote_concurrency).collect::<BTreeSet<_>>();
    let mut remote_events = BinaryHeap::<Reverse<(u64, usize, usize, usize)>>::new();
    let mut remote_clock = RemoteSimulationClock::default();
    let mut remote_in_flight = 0_usize;
    let mut max_remote_in_flight = 0_usize;
    let mut completed_groups = 0_usize;
    let mut group_latencies_ms = Vec::with_capacity(group_count);

    while completed_groups < group_count {
        let next_local_time = local_completion.map(|(time_ms, _)| time_ms);
        let next_remote_time = remote_events.peek().map(|event| event.0 .0);
        now_ms = match (next_local_time, next_remote_time) {
            (Some(local), Some(remote)) => local.min(remote),
            (Some(local), None) => local,
            (None, Some(remote)) => remote,
            (None, None) => panic!("simulation stalled before every group completed"),
        };

        while remote_events
            .peek()
            .is_some_and(|event| event.0 .0 == now_ms)
        {
            let Reverse((_, worker_index, group_index, next_stage_index)) = remote_events
                .pop()
                .expect("peeked remote event must still be present");
            if next_stage_index < groups[group_index].remote_stages.len() {
                schedule_remote_stage(
                    groups,
                    group_index,
                    worker_index,
                    next_stage_index,
                    now_ms,
                    &mut remote_clock,
                    &mut remote_events,
                );
            } else {
                remote_in_flight -= 1;
                completed_groups += 1;
                free_workers.insert(worker_index);
                let ready_at_ms = local_ready_at[group_index]
                    .expect("a completed remote group must have a local-ready timestamp");
                group_latencies_ms.push(now_ms.saturating_sub(ready_at_ms));
            }
        }

        if local_completion.is_some_and(|(time_ms, _)| time_ms == now_ms) {
            let (_, group_index) = local_completion.take().expect("matched local completion");
            local_ready_at[group_index] = Some(now_ms);
            if ready_groups.len() < queue_capacity {
                ready_groups.push_back(group_index);
                local_pipeline_ms = now_ms;
                let next_local_group = group_index + 1;
                if next_local_group < group_count {
                    local_completion = Some((
                        now_ms.saturating_add(groups[next_local_group].local_analysis_ms),
                        next_local_group,
                    ));
                }
            } else {
                blocked_local_group = Some((group_index, now_ms));
            }
        }

        loop {
            let mut changed = false;
            while !ready_groups.is_empty() && !free_workers.is_empty() {
                let group_index = ready_groups
                    .pop_front()
                    .expect("non-empty ready queue must produce a group");
                let worker_index = free_workers
                    .pop_first()
                    .expect("non-empty worker set must produce a worker");
                remote_in_flight += 1;
                max_remote_in_flight = max_remote_in_flight.max(remote_in_flight);
                schedule_remote_stage(
                    groups,
                    group_index,
                    worker_index,
                    0,
                    now_ms,
                    &mut remote_clock,
                    &mut remote_events,
                );
                changed = true;
            }

            if ready_groups.len() < queue_capacity {
                if let Some((group_index, _ready_at_ms)) = blocked_local_group.take() {
                    ready_groups.push_back(group_index);
                    local_pipeline_ms = now_ms;
                    let next_local_group = group_index + 1;
                    if next_local_group < group_count {
                        local_completion = Some((
                            now_ms.saturating_add(groups[next_local_group].local_analysis_ms),
                            next_local_group,
                        ));
                    }
                    changed = true;
                }
            }

            if !changed {
                break;
            }
        }
    }

    group_latencies_ms.sort_unstable();
    SimulationResult {
        remote_concurrency,
        queue_capacity,
        group_count,
        tmdb_request_count,
        artwork_request_count,
        local_compute_ms,
        local_backpressure_ms: local_pipeline_ms.saturating_sub(local_compute_ms),
        local_pipeline_ms,
        remote_pipeline_ms: now_ms,
        pipeline_wall_ms: now_ms,
        group_latency_p50_ms: percentile(&group_latencies_ms, 50),
        group_latency_p95_ms: percentile(&group_latencies_ms, 95),
        max_remote_in_flight,
    }
}

fn percentile(sorted_values: &[u64], percentile: usize) -> u64 {
    assert!(!sorted_values.is_empty());
    assert!((1..=100).contains(&percentile));
    let index = (sorted_values.len() * percentile).div_ceil(100) - 1;
    sorted_values[index]
}

fn emit_simulation_log(name: &str, result: &SimulationResult, baseline_ms: u64) {
    let throughput = result.group_count as f64 * 60_000_f64 / result.pipeline_wall_ms as f64;
    let speedup = baseline_ms as f64 / result.pipeline_wall_ms as f64;
    println!(
        "event=library_scan_performance_simulation simulation=true scenario={name} groups={} tmdb_requests={} artwork_requests={} remote_concurrency={} queue_capacity={} tmdb_min_interval_ms={} local_compute_ms={} local_backpressure_ms={} local_pipeline_ms={} remote_pipeline_ms={} pipeline_wall_ms={} group_latency_p50_ms={} group_latency_p95_ms={} max_remote_in_flight={} throughput_groups_per_min={throughput:.2} speedup={speedup:.2}",
        result.group_count,
        result.tmdb_request_count,
        result.artwork_request_count,
        result.remote_concurrency,
        result.queue_capacity,
        TMDB_REQUEST_MIN_INTERVAL_MS,
        result.local_compute_ms,
        result.local_backpressure_ms,
        result.local_pipeline_ms,
        result.remote_pipeline_ms,
        result.pipeline_wall_ms,
        result.group_latency_p50_ms,
        result.group_latency_p95_ms,
        result.max_remote_in_flight,
    );
}

#[test]
fn selected_remote_pipeline_configuration_is_evidence_backed() {
    let groups = mixed_first_scan_workload();
    let scenarios = [
        ("current-c1-q2", 1, 2),
        ("c2-q4", 2, 4),
        (
            "selected-c4-q2",
            REMOTE_ENRICHMENT_CONCURRENCY,
            REMOTE_ENRICHMENT_QUEUE_CAPACITY,
        ),
        (
            "selected-with-oversized-queue",
            REMOTE_ENRICHMENT_CONCURRENCY,
            16,
        ),
        ("c8-q2", 8, 2),
        ("c8-q16", 8, 16),
    ];
    let results = scenarios
        .iter()
        .map(|(_, concurrency, queue_capacity)| {
            simulate_remote_pipeline(&groups, *concurrency, *queue_capacity)
        })
        .collect::<Vec<_>>();
    let baseline_ms = results[0].pipeline_wall_ms;

    for ((name, _, _), result) in scenarios.iter().zip(&results) {
        emit_simulation_log(name, result, baseline_ms);
    }

    assert_eq!(results[0].group_count, SIMULATED_GROUP_COUNT);
    assert_eq!(results[0].local_compute_ms, 12_000);
    assert!(results[1].pipeline_wall_ms < results[0].pipeline_wall_ms);
    assert!(results[2].pipeline_wall_ms < results[1].pipeline_wall_ms);
    assert_eq!(results[2].pipeline_wall_ms, results[3].pipeline_wall_ms);
    assert!(results[2].group_latency_p95_ms < results[3].group_latency_p95_ms);
    assert_eq!(
        results[2].max_remote_in_flight,
        REMOTE_ENRICHMENT_CONCURRENCY
    );
    assert!(results[4].pipeline_wall_ms < results[3].pipeline_wall_ms);
    assert_eq!(results[4].pipeline_wall_ms, results[5].pipeline_wall_ms);
    assert_eq!(results[4].local_backpressure_ms, 0);
    assert_eq!(results[5].max_remote_in_flight, 8);
}
