use std::collections::HashMap;

use std::time::SystemTime;
use utils::{
    id::{TenantId, TimelineId},
    lsn::Lsn,
};

use super::*;
use chrono::{DateTime, Utc};

#[test]
fn startup_collected_timeline_metrics_before_advancing() {
    let tenant_id = TenantId::generate();
    let timeline_id = TimelineId::generate();

    let mut metrics = Vec::new();
    let cache = HashMap::new();

    let initdb_lsn = Lsn(0x10000);
    let disk_consistent_lsn = Lsn(initdb_lsn.0 * 2);

    let snap = TimelineSnapshot {
        loaded_at: (disk_consistent_lsn, SystemTime::now()),
        last_record_lsn: disk_consistent_lsn,
        current_exact_logical_size: Some(0x42000),
    };

    let now = DateTime::<Utc>::from(SystemTime::now());

    snap.to_metrics(tenant_id, timeline_id, now, &mut metrics, &cache);

    assert_eq!(
        metrics,
        &[
            MetricsKey::written_size_delta(tenant_id, timeline_id).from_previous_up_to(
                snap.loaded_at.1.into(),
                now,
                0
            ),
            MetricsKey::written_size(tenant_id, timeline_id).at(now, disk_consistent_lsn.0),
            MetricsKey::timeline_logical_size(tenant_id, timeline_id).at(now, 0x42000)
        ]
    );
}

#[test]
fn startup_collected_timeline_metrics_second_round() {
    let tenant_id = TenantId::generate();
    let timeline_id = TimelineId::generate();

    let [now, before, init] = time_backwards();

    let now = DateTime::<Utc>::from(now);
    let before = DateTime::<Utc>::from(before);

    let initdb_lsn = Lsn(0x10000);
    let disk_consistent_lsn = Lsn(initdb_lsn.0 * 2);

    let mut metrics = Vec::new();
    let cache = HashMap::from([
        MetricsKey::written_size(tenant_id, timeline_id).at(before, disk_consistent_lsn.0)
    ]);

    let snap = TimelineSnapshot {
        loaded_at: (disk_consistent_lsn, init),
        last_record_lsn: disk_consistent_lsn,
        current_exact_logical_size: Some(0x42000),
    };

    snap.to_metrics(tenant_id, timeline_id, now, &mut metrics, &cache);

    assert_eq!(
        metrics,
        &[
            MetricsKey::written_size_delta(tenant_id, timeline_id)
                .from_previous_up_to(before, now, 0),
            MetricsKey::written_size(tenant_id, timeline_id).at(now, disk_consistent_lsn.0),
            MetricsKey::timeline_logical_size(tenant_id, timeline_id).at(now, 0x42000)
        ]
    );
}

#[test]
fn startup_collected_timeline_metrics_nth_round_at_same_lsn() {
    let tenant_id = TenantId::generate();
    let timeline_id = TimelineId::generate();

    let [now, just_before, before, init] = time_backwards();

    let now = DateTime::<Utc>::from(now);
    let just_before = DateTime::<Utc>::from(just_before);
    let before = DateTime::<Utc>::from(before);

    let initdb_lsn = Lsn(0x10000);
    let disk_consistent_lsn = Lsn(initdb_lsn.0 * 2);

    let mut metrics = Vec::new();
    let cache = HashMap::from([
        // at t=before was the last time the last_record_lsn changed
        MetricsKey::written_size(tenant_id, timeline_id).at(before, disk_consistent_lsn.0),
        // end time of this event is used for the next ones
        MetricsKey::written_size_delta(tenant_id, timeline_id).from_previous_up_to(
            before,
            just_before,
            0,
        ),
    ]);

    let snap = TimelineSnapshot {
        loaded_at: (disk_consistent_lsn, init),
        last_record_lsn: disk_consistent_lsn,
        current_exact_logical_size: Some(0x42000),
    };

    snap.to_metrics(tenant_id, timeline_id, now, &mut metrics, &cache);

    assert_eq!(
        metrics,
        &[
            MetricsKey::written_size_delta(tenant_id, timeline_id).from_previous_up_to(
                just_before,
                now,
                0
            ),
            MetricsKey::written_size(tenant_id, timeline_id).at(now, disk_consistent_lsn.0),
            MetricsKey::timeline_logical_size(tenant_id, timeline_id).at(now, 0x42000)
        ]
    );
}

#[test]
fn metric_image_stability() {
    // it is important that these strings stay as they are

    let tenant_id = TenantId::from_array([0; 16]);
    let timeline_id = TimelineId::from_array([0xff; 16]);

    let now = DateTime::parse_from_rfc3339("2023-09-15T00:00:00.123456789Z").unwrap();
    let before = DateTime::parse_from_rfc3339("2023-09-14T00:00:00.123456789Z").unwrap();

    let [now, before] = [DateTime::<Utc>::from(now), DateTime::from(before)];

    let examples = [
        (
            line!(),
            MetricsKey::written_size(tenant_id, timeline_id).at(now, 0),
            r#"{"type":"absolute","time":"2023-09-15T00:00:00.123456789Z","metric":"written_size","idempotency_key":"2023-09-15 00:00:00.123456789 UTC-1-0000","value":0,"tenant_id":"00000000000000000000000000000000","timeline_id":"ffffffffffffffffffffffffffffffff"}"#,
        ),
        (
            line!(),
            MetricsKey::written_size_delta(tenant_id, timeline_id)
                .from_previous_up_to(before, now, 0),
            r#"{"type":"incremental","start_time":"2023-09-14T00:00:00.123456789Z","stop_time":"2023-09-15T00:00:00.123456789Z","metric":"written_data_bytes_delta","idempotency_key":"2023-09-15 00:00:00.123456789 UTC-1-0000","value":0,"tenant_id":"00000000000000000000000000000000","timeline_id":"ffffffffffffffffffffffffffffffff"}"#,
        ),
        (
            line!(),
            MetricsKey::timeline_logical_size(tenant_id, timeline_id).at(now, 0),
            r#"{"type":"absolute","time":"2023-09-15T00:00:00.123456789Z","metric":"timeline_logical_size","idempotency_key":"2023-09-15 00:00:00.123456789 UTC-1-0000","value":0,"tenant_id":"00000000000000000000000000000000","timeline_id":"ffffffffffffffffffffffffffffffff"}"#,
        ),
        (
            line!(),
            MetricsKey::remote_storage_size(tenant_id).at(now, 0),
            r#"{"type":"absolute","time":"2023-09-15T00:00:00.123456789Z","metric":"remote_storage_size","idempotency_key":"2023-09-15 00:00:00.123456789 UTC-1-0000","value":0,"tenant_id":"00000000000000000000000000000000"}"#,
        ),
        (
            line!(),
            MetricsKey::resident_size(tenant_id).at(now, 0),
            r#"{"type":"absolute","time":"2023-09-15T00:00:00.123456789Z","metric":"resident_size","idempotency_key":"2023-09-15 00:00:00.123456789 UTC-1-0000","value":0,"tenant_id":"00000000000000000000000000000000"}"#,
        ),
        (
            line!(),
            MetricsKey::synthetic_size(tenant_id).at(now, 1),
            r#"{"type":"absolute","time":"2023-09-15T00:00:00.123456789Z","metric":"synthetic_storage_size","idempotency_key":"2023-09-15 00:00:00.123456789 UTC-1-0000","value":1,"tenant_id":"00000000000000000000000000000000"}"#,
        ),
    ];

    let idempotency_key = consumption_metrics::IdempotencyKey::for_tests(now, "1", 0);

    for (line, (key, (kind, value)), expected) in examples {
        let e = consumption_metrics::Event {
            kind,
            metric: key.metric,
            idempotency_key: idempotency_key.to_string(),
            value,
            extra: Ids {
                tenant_id: key.tenant_id,
                timeline_id: key.timeline_id,
            },
        };
        let actual = serde_json::to_string(&e).unwrap();
        assert_eq!(expected, actual, "example from line {line}");
    }
}

#[test]
fn post_restart_written_sizes_with_rolled_back_last_record_lsn() {
    // it can happen that we lose the inmemorylayer but have previously sent metrics and we
    // should never go backwards

    let tenant_id = TenantId::generate();
    let timeline_id = TimelineId::generate();

    let [later, now, at_restart] = time_backwards();

    // FIXME: tests would be so much easier if we did not need to juggle back and forth
    // SystemTime and DateTime::<Utc> ... Could do the conversion only at upload time?
    let now = DateTime::<Utc>::from(now);
    let later = DateTime::<Utc>::from(later);
    let before_restart = at_restart - std::time::Duration::from_secs(5 * 60);
    let way_before = before_restart - std::time::Duration::from_secs(10 * 60);
    let before_restart = DateTime::<Utc>::from(before_restart);
    let way_before = DateTime::<Utc>::from(way_before);

    let snap = TimelineSnapshot {
        loaded_at: (Lsn(50), at_restart),
        last_record_lsn: Lsn(50),
        current_exact_logical_size: None,
    };

    let mut cache = HashMap::from([
        MetricsKey::written_size(tenant_id, timeline_id).at(before_restart, 100),
        MetricsKey::written_size_delta(tenant_id, timeline_id).from_previous_up_to(
            way_before,
            before_restart,
            // not taken into account, but the timestamps are important
            999_999_999,
        ),
    ]);

    let mut metrics = Vec::new();
    snap.to_metrics(tenant_id, timeline_id, now, &mut metrics, &cache);

    assert_eq!(
        metrics,
        &[
            MetricsKey::written_size_delta(tenant_id, timeline_id).from_previous_up_to(
                before_restart,
                now,
                0
            ),
            MetricsKey::written_size(tenant_id, timeline_id).at(now, 100),
        ]
    );

    // now if we cache these metrics, and re-run while "still in recovery"
    cache.extend(metrics.drain(..));

    // "still in recovery", because our snapshot did not change
    snap.to_metrics(tenant_id, timeline_id, later, &mut metrics, &cache);

    assert_eq!(
        metrics,
        &[
            MetricsKey::written_size_delta(tenant_id, timeline_id)
                .from_previous_up_to(now, later, 0),
            MetricsKey::written_size(tenant_id, timeline_id).at(later, 100),
        ]
    );
}

#[test]
fn post_restart_current_exact_logical_size_uses_cached() {
    let tenant_id = TenantId::generate();
    let timeline_id = TimelineId::generate();

    let [now, at_restart] = time_backwards();

    let now = DateTime::<Utc>::from(now);
    let before_restart = at_restart - std::time::Duration::from_secs(5 * 60);
    let before_restart = DateTime::<Utc>::from(before_restart);

    let snap = TimelineSnapshot {
        loaded_at: (Lsn(50), at_restart),
        last_record_lsn: Lsn(50),
        current_exact_logical_size: None,
    };

    let cache = HashMap::from([
        MetricsKey::timeline_logical_size(tenant_id, timeline_id).at(before_restart, 100)
    ]);

    let mut metrics = Vec::new();
    snap.to_metrics(tenant_id, timeline_id, now, &mut metrics, &cache);

    metrics.retain(|(key, _)| key.metric == Name::LogicalSize);

    assert_eq!(
        metrics,
        &[MetricsKey::timeline_logical_size(tenant_id, timeline_id).at(now, 100)]
    );
}

#[test]
fn post_restart_synthetic_size_uses_cached_if_available() {
    let tenant_id = TenantId::generate();

    let ts = TenantSnapshot {
        resident_size: 1000,
        remote_size: 1000,
        // not yet calculated
        synthetic_size: 0,
    };

    let now = SystemTime::now();
    let before_restart = DateTime::<Utc>::from(now - std::time::Duration::from_secs(5 * 60));
    let now = DateTime::<Utc>::from(now);

    let cached = HashMap::from([MetricsKey::synthetic_size(tenant_id).at(before_restart, 1000)]);

    let mut metrics = Vec::new();
    ts.to_metrics(tenant_id, now, &cached, &mut metrics);

    assert_eq!(
        metrics,
        &[
            MetricsKey::remote_storage_size(tenant_id).at(now, 1000),
            MetricsKey::resident_size(tenant_id).at(now, 1000),
            MetricsKey::synthetic_size(tenant_id).at(now, 1000),
        ]
    );
}

#[test]
fn post_restart_synthetic_size_is_not_sent_when_not_cached() {
    let tenant_id = TenantId::generate();

    let ts = TenantSnapshot {
        resident_size: 1000,
        remote_size: 1000,
        // not yet calculated
        synthetic_size: 0,
    };

    let now = SystemTime::now();
    let now = DateTime::<Utc>::from(now);

    let cached = HashMap::new();

    let mut metrics = Vec::new();
    ts.to_metrics(tenant_id, now, &cached, &mut metrics);

    assert_eq!(
        metrics,
        &[
            MetricsKey::remote_storage_size(tenant_id).at(now, 1000),
            MetricsKey::resident_size(tenant_id).at(now, 1000),
            // no synthetic size here
        ]
    );
}

fn time_backwards<const N: usize>() -> [std::time::SystemTime; N] {
    let mut times = [std::time::SystemTime::UNIX_EPOCH; N];
    times[0] = std::time::SystemTime::now();
    for behind in 1..N {
        times[behind] = times[0] - std::time::Duration::from_secs(behind as u64);
    }

    times
}
