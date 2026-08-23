use pulsedag_storage::Storage;
use std::time::{SystemTime, UNIX_EPOCH};

const AUTOMATIC_RUNTIME_EVENT_CAP: usize = 2_000;

fn temp_db_path() -> String {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir()
        .join(format!("pulsedag-task29-resource-bounds-{unique}"))
        .to_string_lossy()
        .into_owned()
}

#[test]
fn task29_runtime_event_storage_retention_is_bounded() {
    let path = temp_db_path();
    let storage = Storage::open(&path).expect("open Task29 storage fixture");

    for index in 0..(AUTOMATIC_RUNTIME_EVENT_CAP + 5) {
        storage
            .append_runtime_event("info", "task29_resource_bound", &format!("event-{index}"))
            .expect("append runtime event");
    }

    let retained = storage
        .list_runtime_events(usize::MAX)
        .expect("list automatically retained events");
    assert_eq!(retained.len(), AUTOMATIC_RUNTIME_EVENT_CAP);
    assert!(retained
        .iter()
        .any(|event| event.message == format!("event-{}", AUTOMATIC_RUNTIME_EVENT_CAP + 4)));

    let removed = storage
        .prune_runtime_events(128)
        .expect("tighten runtime-event retention");
    assert_eq!(removed, AUTOMATIC_RUNTIME_EVENT_CAP - 128);
    assert_eq!(
        storage
            .list_runtime_events(usize::MAX)
            .expect("list tightly retained events")
            .len(),
        128
    );
    assert_eq!(
        storage
            .list_runtime_events(17)
            .expect("bounded runtime-event query")
            .len(),
        17
    );

    drop(storage);
    let _ = std::fs::remove_dir_all(path);
}
