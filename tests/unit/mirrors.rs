use osu_collect::mirrors::{CatboyRegion, MirrorEndpoint, MirrorKind};

#[test]
fn builtin_nerinyan_generates_valid_url() {
    let endpoint = MirrorEndpoint::builtin(MirrorKind::Nerinyan, false).unwrap();
    let url = endpoint.url_for(12345);
    assert!(url.contains("12345"));
    assert!(url.starts_with("https://"));
}

#[test]
fn builtin_nerinyan_no_video() {
    let with_video = MirrorEndpoint::builtin(MirrorKind::Nerinyan, false).unwrap();
    let no_video = MirrorEndpoint::builtin(MirrorKind::Nerinyan, true).unwrap();
    let url_video = with_video.url_for(100);
    let url_no_video = no_video.url_for(100);
    assert_ne!(url_video, url_no_video);
}

#[test]
fn custom_mirror_requires_id_placeholder() {
    let result = MirrorEndpoint::custom("https://example.com/download");
    assert!(result.is_err());
}

#[test]
fn custom_mirror_requires_http() {
    let result = MirrorEndpoint::custom("ftp://example.com/{id}");
    assert!(result.is_err());
}

#[test]
fn custom_mirror_valid() {
    let endpoint = MirrorEndpoint::custom("https://example.com/d/{id}").unwrap();
    assert_eq!(endpoint.kind, MirrorKind::Custom);
    let url = endpoint.url_for(99999);
    assert_eq!(url, "https://example.com/d/99999");
}

#[test]
fn catboy_regions_produce_different_urls() {
    let central =
        MirrorEndpoint::builtin(MirrorKind::Catboy(CatboyRegion::Central), false).unwrap();
    let us = MirrorEndpoint::builtin(MirrorKind::Catboy(CatboyRegion::Us), false).unwrap();
    let asia = MirrorEndpoint::builtin(MirrorKind::Catboy(CatboyRegion::Asia), false).unwrap();

    let url_central = central.url_for(1);
    let url_us = us.url_for(1);
    let url_asia = asia.url_for(1);

    assert!(url_central != url_us || url_us != url_asia);
}

#[test]
fn display_name_returns_static_str() {
    let endpoint = MirrorEndpoint::builtin(MirrorKind::Nerinyan, false).unwrap();
    let name = endpoint.display_name();
    assert!(!name.is_empty());
}

#[test]
fn to_mirror_roundtrip() {
    let endpoint = MirrorEndpoint::builtin(MirrorKind::Nerinyan, false).unwrap();
    let mirror = endpoint.to_mirror();
    let back = MirrorEndpoint::from(mirror);
    assert_eq!(back.kind, MirrorKind::Nerinyan);
}

#[test]
fn official_mirror_roundtrip_preserves_auth_headers() {
    let endpoint = MirrorEndpoint::official("abc123");
    let back = MirrorEndpoint::from(endpoint.to_mirror());
    let headers = back.headers.expect("official headers");

    assert_eq!(
        headers
            .get(reqwest::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok()),
        Some("Bearer abc123")
    );
    assert_eq!(
        headers
            .get(reqwest::header::ACCEPT)
            .and_then(|value| value.to_str().ok()),
        Some("application/json")
    );
}
