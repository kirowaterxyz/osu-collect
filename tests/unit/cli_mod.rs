#[tokio::test]
async fn spawn_blocking_join_error_is_not_silently_dropped() {
    let result = tokio::task::spawn_blocking(|| {
        panic!("simulated save failure");
    })
    .await;

    assert!(result.is_err(), "panicking task must yield JoinError");
    let err = result.unwrap_err();
    assert!(err.is_panic());
}
