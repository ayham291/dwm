use dwm_core::{
    Client, ClientId, Command, Config, Monitor, Rect, TagMask, scratch_bit,
};

fn screen() -> Rect {
    Rect::new(0, 0, 1920, 1080)
}

#[test]
fn toggle_scratch_spawns_when_missing() {
    let mut m = Monitor::from_config(&Config::default(), screen());
    let frame = m.apply(Command::ToggleScratch { index: 0 });
    assert!(frame.spawn.is_some());
    assert!(m.tagset().contains(scratch_bit(0)));
}

#[test]
fn toggle_scratch_hides_and_shows_existing() {
    let mut m = Monitor::from_config(&Config::default(), screen());
    let mut c = Client::new(ClientId(0), "spterm");
    c.tags = scratch_bit(0);
    c.floating = true;
    let id = m.manage(c);
    // Ensure scratch tag is in the view.
    m.apply(Command::ToggleScratch { index: 0 });
    assert!(m.client(id).unwrap().is_visible(m.tagset()));

    // Hide
    let frame = m.apply(Command::ToggleScratch { index: 0 });
    assert!(frame.spawn.is_none());
    assert!(!m.tagset().contains(scratch_bit(0)));
    assert!(!m.client(id).unwrap().is_visible(m.tagset()));

    // Show again
    m.apply(Command::ToggleScratch { index: 0 });
    assert!(m.tagset().contains(scratch_bit(0)));
    assert!(m.client(id).unwrap().is_visible(m.tagset()));
}

#[test]
fn scratchpad_rule_floats_and_centers() {
    let mut m = Monitor::from_config(&Config::default(), screen());
    let id = m.manage(Client::new(ClientId(0), "pending"));
    assert!(m.apply_scratchpad_rule(id, "spterm — foot", "foot"));
    let c = m.client(id).unwrap();
    assert_eq!(c.tags, scratch_bit(0));
    assert!(c.floating);
    assert!(c.float_geom.w > 0 && c.float_geom.h > 0);
    // Roughly centered in the window area.
    assert!(c.float_geom.x > m.window_area.x);
    assert!(c.float_geom.y >= m.window_area.y);
}

#[test]
fn normal_clients_do_not_inherit_scratch_tags() {
    let mut m = Monitor::from_config(&Config::default(), screen());
    m.apply(Command::ToggleScratch { index: 0 }); // view includes scratch bit
    let id = m.manage(Client::new(ClientId(0), "regular"));
    assert!(!m.client(id).unwrap().tags.contains(TagMask::SCRATCH_MASK));
}

#[test]
fn bar_model_hides_vacant_tags() {
    let mut m = Monitor::from_config(&Config::default(), screen());
    m.manage(Client::new(ClientId(0), "a")); // on tag 0
    let bar = m.bar_model();
    assert!(bar.tags.iter().any(|t| t.label == "1" && t.selected));
    assert!(!bar.tags.iter().any(|t| t.label == "5"));
}
