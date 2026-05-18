use super::resolve_selective_with;
use crate::core::collection::{
    CollectionService,
    model::{Beatmap, Beatmapset, Collection, Uploader},
};
use crate::download::{DownloadEvent, SelectiveDownloadCollection};
use crate::utils;
use std::sync::{Arc, Mutex};

struct MockService {
    responses: Vec<(u32, Result<Collection, &'static str>)>,
}

impl CollectionService for MockService {
    async fn fetch_collection(&self, id: u32) -> utils::Result<Collection> {
        let response = self
            .responses
            .iter()
            .find(|(cid, _)| *cid == id)
            .map(|(_, r)| r.clone())
            .unwrap_or(Err("missing"));
        response.map_err(utils::AppError::other)
    }
}

fn beatmapset(id: u32) -> Beatmapset {
    Beatmapset {
        id,
        beatmaps: vec![Beatmap {
            id,
            checksum: "abc".into(),
        }],
    }
}

fn collection(id: u32, name: &str, ids: &[u32]) -> Collection {
    Collection {
        id,
        name: name.into(),
        uploader: Uploader {
            id: 0,
            username: "u".into(),
        },
        beatmapsets: ids.iter().copied().map(beatmapset).collect(),
    }
}

#[tokio::test]
async fn resolve_selective_dedupes_overlapping_beatmapsets() {
    let service = MockService {
        responses: vec![
            (1, Ok(collection(1, "alpha", &[10, 11]))),
            (2, Ok(collection(2, "beta", &[10, 12]))),
        ],
    };
    let requested = vec![
        SelectiveDownloadCollection {
            id: 1,
            name: String::new(),
            beatmapset_ids: vec![10, 11],
        },
        SelectiveDownloadCollection {
            id: 2,
            name: String::new(),
            beatmapset_ids: vec![10, 12],
        },
    ];
    let emit = |_event| {};
    let (selected, resolved, names) =
        resolve_selective_with(&service, &[1, 2], requested, &[10, 11, 12], 7, &emit)
            .await
            .expect("resolve must succeed");

    let mut bs_ids: Vec<u32> = selected.beatmapsets.iter().map(|b| b.id).collect();
    bs_ids.sort_unstable();
    assert_eq!(bs_ids, vec![10, 11, 12]);
    assert_eq!(names, vec!["alpha".to_string(), "beta".to_string()]);
    assert_eq!(resolved.len(), 2);
}

#[tokio::test]
async fn resolve_selective_progress_is_monotonic() {
    use std::time::Duration;
    use tokio::time::sleep;

    struct DelayedService {
        responses: Vec<(u32, Collection, Duration)>,
    }
    impl CollectionService for DelayedService {
        async fn fetch_collection(&self, id: u32) -> utils::Result<Collection> {
            let (_, ref c, delay) = *self
                .responses
                .iter()
                .find(|(cid, _, _)| *cid == id)
                .unwrap();
            sleep(delay).await;
            Ok(c.clone())
        }
    }

    let service = DelayedService {
        responses: vec![
            (1, collection(1, "alpha", &[10]), Duration::from_millis(60)),
            (2, collection(2, "beta", &[11]), Duration::from_millis(10)),
            (3, collection(3, "gamma", &[12]), Duration::from_millis(30)),
        ],
    };
    let requested = vec![
        SelectiveDownloadCollection {
            id: 1,
            name: String::new(),
            beatmapset_ids: vec![10],
        },
        SelectiveDownloadCollection {
            id: 2,
            name: String::new(),
            beatmapset_ids: vec![11],
        },
        SelectiveDownloadCollection {
            id: 3,
            name: String::new(),
            beatmapset_ids: vec![12],
        },
    ];
    let events = Arc::new(Mutex::new(Vec::<u32>::new()));
    let events_inner = Arc::clone(&events);
    let emit = move |event: DownloadEvent| {
        if let DownloadEvent::ResolveProgress { current, .. } = event {
            events_inner.lock().unwrap().push(current);
        }
    };

    resolve_selective_with(&service, &[1, 2, 3], requested, &[10, 11, 12], 7, &emit)
        .await
        .expect("resolve must succeed");

    let observed = events.lock().unwrap().clone();
    assert_eq!(observed, vec![0, 1, 2, 3]);
}
