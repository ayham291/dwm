use dwm_core::{
    ArrangeParams, Client, ClientId, Command, Config, LayoutKind, Monitor, Rect, TagMask,
    arrange,
};

fn screen() -> Rect {
    Rect::new(0, 0, 1920, 1080)
}

fn mon_with_n(n: usize) -> Monitor {
    let mut m = Monitor::from_config(&Config::default(), screen());
    for i in 0..n {
        m.manage(Client::new(ClientId(0), format!("win-{i}")));
    }
    m
}

#[test]
fn tile_one_client_fills_window_area_minus_gaps_and_border() {
    let mut m = mon_with_n(1);
    let (geoms, sym) = m.arrange();
    assert_eq!(sym, "[]=");
    assert_eq!(geoms.len(), 1);
    let (_, r) = geoms[0];
    // window area excludes top bar (24px); outer gaps 10/30; border 3*2
    assert!(r.w > 0 && r.h > 0);
    assert!(r.x >= 0);
    assert!(r.y >= 24);
    assert!(r.w + r.x <= 1920);
    assert!(r.h + r.y <= 1080);
}

#[test]
fn tile_two_clients_master_stack_split_by_mfact() {
    let mut m = mon_with_n(2);
    m.mfact = 0.55;
    let (geoms, _) = m.arrange();
    assert_eq!(geoms.len(), 2);
    let (_, master) = geoms[0];
    let (_, stack) = geoms[1];
    assert!(master.x < stack.x, "master should be on the left");
    assert_eq!(master.y, stack.y);
    // Master is roughly mfact of usable width.
    assert!(master.w > stack.w);
}

#[test]
fn bstack_puts_stack_below_master() {
    let mut m = mon_with_n(2);
    m.layout = LayoutKind::BottomStack;
    let (geoms, sym) = m.arrange();
    assert_eq!(sym, "TTT");
    let (_, master) = geoms[0];
    let (_, stack) = geoms[1];
    assert!(stack.y > master.y);
}

#[test]
fn monocle_stacks_all_same_geometry() {
    let mut m = mon_with_n(3);
    m.layout = LayoutKind::Monocle;
    let (geoms, sym) = m.arrange();
    assert_eq!(sym, "[3]");
    assert_eq!(geoms[0].1, geoms[1].1);
    assert_eq!(geoms[1].1, geoms[2].1);
}

#[test]
fn deck_symbol_shows_stack_count() {
    let mut m = mon_with_n(4);
    m.nmaster = 1;
    m.layout = LayoutKind::Deck;
    let (geoms, sym) = m.arrange();
    assert_eq!(sym, "D 3");
    // Stack clients share the same rect.
    assert_eq!(geoms[1].1, geoms[2].1);
    assert_eq!(geoms[2].1, geoms[3].1);
}

#[test]
fn floating_layout_does_not_tile() {
    let mut m = mon_with_n(2);
    m.layout = LayoutKind::Floating;
    for c in &mut m.clients {
        c.floating = true;
    }
    let (geoms, sym) = m.arrange();
    assert_eq!(sym, "><>");
    // Floating clients report float_geom.
    assert_eq!(geoms.len(), 2);
    for (_, r) in &geoms {
        assert_eq!(*r, Rect::new(100, 100, 800, 600));
    }
}

#[test]
fn focus_stack_cycles() {
    let mut m = mon_with_n(3);
    let first = m.sel.unwrap();
    m.apply(Command::FocusStack { delta: 1 });
    let second = m.sel.unwrap();
    assert_ne!(first, second);
    m.apply(Command::FocusStack { delta: 1 });
    m.apply(Command::FocusStack { delta: 1 });
    assert_eq!(m.sel.unwrap(), first);
}

#[test]
fn tags_hide_clients_on_other_views() {
    let mut m = mon_with_n(2);
    let a = m.clients[0].id;
    let b = m.clients[1].id;
    m.client_mut(b).unwrap().tags = TagMask::bit(1);
    m.apply(Command::View {
        tags: TagMask::bit(0),
    });
    let visible: Vec<_> = m.visible_clients().iter().map(|c| c.id).collect();
    assert_eq!(visible, vec![a]);
    m.apply(Command::View {
        tags: TagMask::bit(1),
    });
    let visible: Vec<_> = m.visible_clients().iter().map(|c| c.id).collect();
    assert_eq!(visible, vec![b]);
}

#[test]
fn sticky_client_visible_on_all_tags() {
    let mut m = mon_with_n(1);
    let id = m.clients[0].id;
    m.client_mut(id).unwrap().sticky = true;
    m.apply(Command::View {
        tags: TagMask::bit(5),
    });
    assert!(m.visible_clients().iter().any(|c| c.id == id));
}

#[test]
fn shift_view_skips_empty_tags() {
    let mut m = mon_with_n(0);
    let mut c0 = Client::new(ClientId(0), "a");
    c0.tags = TagMask::bit(0);
    let mut c2 = Client::new(ClientId(0), "b");
    c2.tags = TagMask::bit(2);
    m.manage(c0);
    m.manage(c2);
    m.apply(Command::View {
        tags: TagMask::bit(0),
    });
    m.apply(Command::ShiftView { delta: 1 });
    assert_eq!(m.tagset(), TagMask::bit(2));
}

#[test]
fn push_stack_zoom_moves_to_master() {
    let mut m = mon_with_n(3);
    // Focus the last client, then zoom.
    let last = m.clients[2].id;
    m.focus(Some(last));
    m.apply(Command::PushStack { delta: 0 });
    assert_eq!(m.clients[0].id, last);
}

#[test]
fn toggle_gaps_changes_geometry() {
    let mut m = mon_with_n(1);
    let (_, with_gaps) = m.arrange();
    let w1 = m.clients[0].geom.w;
    m.apply(Command::ToggleGaps);
    let _ = m.arrange();
    let w2 = m.clients[0].geom.w;
    assert!(w2 > w1, "disabling gaps should enlarge the client ({w2} > {w1})");
    let _ = with_gaps;
}

#[test]
fn mfact_and_nmaster_adjust() {
    let mut m = mon_with_n(3);
    m.apply(Command::SetMFact { delta: 0.10 });
    assert!((m.mfact - 0.65).abs() < f32::EPSILON);
    m.apply(Command::IncNMaster { delta: 1 });
    assert_eq!(m.nmaster, 2);
    let (geoms, _) = m.arrange();
    // With nmaster=2 and 3 clients, two on left stack vertically, one on right.
    assert_eq!(geoms[0].1.x, geoms[1].1.x);
    assert!(geoms[2].1.x > geoms[0].1.x);
}

#[test]
fn kill_and_quit_commands() {
    let mut m = mon_with_n(1);
    let id = m.sel.unwrap();
    let frame = m.apply(Command::KillFocused);
    assert_eq!(frame.kill, Some(id));
    let frame = m.apply(Command::Quit);
    assert!(frame.quit);
}

#[test]
fn set_layout_toggle_restores_previous() {
    let mut m = mon_with_n(1);
    m.apply(Command::SetLayout(LayoutKind::Monocle));
    assert_eq!(m.layout, LayoutKind::Monocle);
    m.apply(Command::SetLayout(LayoutKind::Monocle));
    assert_eq!(m.layout, LayoutKind::Tile);
}

#[test]
fn arrange_helper_empty_tiled() {
    let params = ArrangeParams {
        window_area: screen(),
        mfact: 0.55,
        nmaster: 1,
        gaps: Default::default(),
        enable_gaps: true,
        smart_gaps: false,
        border_px: 3,
        layout: LayoutKind::Tile,
    };
    let (rects, sym) = arrange(&[], params);
    assert!(rects.is_empty());
    assert_eq!(sym, "[]=");
}

#[test]
fn spiral_and_dwindle_produce_n_rects() {
    for kind in [LayoutKind::Spiral, LayoutKind::Dwindle, LayoutKind::CenteredMaster] {
        let mut m = mon_with_n(5);
        m.layout = kind;
        let (geoms, _) = m.arrange();
        assert_eq!(geoms.len(), 5, "{kind:?}");
        for (_, r) in &geoms {
            assert!(r.w > 0 && r.h > 0, "{kind:?} zero-size: {r:?}");
        }
    }
}

#[test]
fn occupied_tags_mask() {
    let mut m = mon_with_n(0);
    let mut a = Client::new(ClientId(0), "a");
    a.tags = TagMask::bit(0).union(TagMask::bit(3));
    m.manage(a);
    assert_eq!(
        m.occupied_tags(),
        TagMask::bit(0).union(TagMask::bit(3))
    );
}

#[test]
fn tag_selected_moves_window() {
    let mut m = mon_with_n(1);
    let id = m.sel.unwrap();
    m.apply(Command::Tag {
        tags: TagMask::bit(4),
    });
    assert_eq!(m.client(id).unwrap().tags, TagMask::bit(4));
}
