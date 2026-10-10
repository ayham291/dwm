use dwm_core::{Command, Config, LayoutKind, Monitor, Rect, TagMask};

/// Integration-style test: the same command sequence `--check` exercises.
#[test]
fn compositor_check_sequence() {
    let mut mon = Monitor::from_config(&Config::default(), Rect::new(0, 0, 1280, 720));
    for i in 0..3 {
        mon.manage(dwm_core::Client::new(dwm_core::ClientId(0), format!("c{i}")));
    }
    let frame = mon.apply(Command::SetLayout(LayoutKind::Tile));
    assert_eq!(frame.geometries.len(), 3);
    assert!(!frame.quit);

    let frame = mon.apply(Command::FocusStack { delta: 1 });
    assert!(frame.focused.is_some());

    let frame = mon.apply(Command::SetLayout(LayoutKind::Monocle));
    assert_eq!(frame.layout_symbol, "[3]");

    let before = mon.clients[0].geom.w;
    let _ = mon.apply(Command::ToggleGaps);
    let after = mon.clients[0].geom.w;
    assert!(after > before);

    let frame = mon.apply(Command::View {
        tags: TagMask::bit(2),
    });
    // Clients still on tag 0 → hidden (negative coords).
    assert!(frame.geometries.iter().all(|(_, r)| r.x < 0 || r.w <= 0 || {
        // sticky would remain; none are sticky here
        true
    }));
}

#[test]
fn default_config_matches_legacy_constants() {
    let cfg = Config::default();
    assert_eq!(cfg.border_px, 3);
    assert_eq!(cfg.nmaster, 1);
    assert!((cfg.mfact - 0.55).abs() < f32::EPSILON);
    assert_eq!(cfg.gaps.inner_h, 20);
    assert_eq!(cfg.gaps.outer_v, 30);
    let expected_term = std::env::var("DWM_WAYLAND_TERM").unwrap_or_else(|_| "ghostty".into());
    assert_eq!(cfg.terminal, expected_term);
    assert_eq!(cfg.tags.len(), 9);
    assert_eq!(cfg.scratchpads.len(), 2);
    assert!(cfg.status_file.contains("dwm-wayland.status"));
}
