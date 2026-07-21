pub fn get(state: &str) -> Option<&'static [u8]> {
    match state {
        "idle" => Some(include_bytes!("../assets/layout-guides/idle.png")),
        "running-right" => Some(include_bytes!("../assets/layout-guides/running-right.png")),
        "running-left" => Some(include_bytes!("../assets/layout-guides/running-left.png")),
        "waving" => Some(include_bytes!("../assets/layout-guides/waving.png")),
        "jumping" => Some(include_bytes!("../assets/layout-guides/jumping.png")),
        "failed" => Some(include_bytes!("../assets/layout-guides/failed.png")),
        "waiting" => Some(include_bytes!("../assets/layout-guides/waiting.png")),
        "running" => Some(include_bytes!("../assets/layout-guides/running.png")),
        "review" => Some(include_bytes!("../assets/layout-guides/review.png")),
        _ => None,
    }
}
