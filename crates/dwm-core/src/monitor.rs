use crate::config::{Config, ScratchpadConfig};
use crate::layout::{self, ArrangeParams, LayoutKind, Rect};
use crate::tags::{NUM_TAGS, TagMask};

/// Opaque client handle used by the compositor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClientId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Gaps {
    pub inner_h: u32,
    pub inner_v: u32,
    pub outer_h: u32,
    pub outer_v: u32,
}

impl Default for Gaps {
    fn default() -> Self {
        Self {
            inner_h: 20,
            inner_v: 10,
            outer_h: 10,
            outer_v: 30,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Client {
    pub id: ClientId,
    pub name: String,
    pub app_id: String,
    pub tags: TagMask,
    pub floating: bool,
    pub fullscreen: bool,
    pub sticky: bool,
    pub urgent: bool,
    pub border: i32,
    pub geom: Rect,
    pub float_geom: Rect,
}

impl Client {
    pub fn new(id: ClientId, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            app_id: String::new(),
            tags: TagMask::bit(0),
            floating: false,
            fullscreen: false,
            sticky: false,
            urgent: false,
            border: 3,
            geom: Rect::default(),
            float_geom: Rect::new(100, 100, 800, 600),
        }
    }

    pub fn is_visible(&self, tagset: TagMask) -> bool {
        self.sticky || tagset.contains(self.tags)
    }
}

/// Side-effect hints returned alongside arrange (kept for API symmetry).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MonitorAction {
    None,
    FocusChanged,
    ClientKilled,
}

/// One tag cell for the status bar.
#[derive(Debug, Clone)]
pub struct BarTag {
    pub label: String,
    pub selected: bool,
    pub occupied: bool,
    pub urgent: bool,
}

/// Snapshot of what the status bar should paint.
#[derive(Debug, Clone)]
pub struct BarModel {
    pub tags: Vec<BarTag>,
    pub layout_symbol: String,
    pub title: String,
    pub status: String,
    pub height: i32,
    pub width: i32,
    pub top: bool,
}

/// One output / monitor worth of dwm state.
#[derive(Debug, Clone)]
pub struct Monitor {
    pub mfact: f32,
    pub nmaster: usize,
    pub screen: Rect,
    pub window_area: Rect,
    pub gaps: Gaps,
    pub enable_gaps: bool,
    pub smart_gaps: bool,
    pub show_bar: bool,
    pub top_bar: bool,
    pub bar_height: i32,
    pub border_px: i32,
    pub layout: LayoutKind,
    pub prev_layout: LayoutKind,
    tagset: [TagMask; 2],
    seltags: usize,
    pub clients: Vec<Client>,
    /// Focus stack: most recently focused first.
    pub stack: Vec<ClientId>,
    pub sel: Option<ClientId>,
    next_id: u64,
    default_gaps: Gaps,
    pub tag_labels: Vec<String>,
    pub scratchpads: Vec<ScratchpadConfig>,
    pub status_text: String,
    pub layout_symbol: String,
}

impl Monitor {
    pub fn from_config(cfg: &Config, screen: Rect) -> Self {
        let bar_h = if cfg.show_bar { cfg.bar_height } else { 0 };
        let window_area = if cfg.top_bar {
            Rect::new(screen.x, screen.y + bar_h, screen.w, screen.h - bar_h)
        } else {
            Rect::new(screen.x, screen.y, screen.w, screen.h - bar_h)
        };
        Self {
            mfact: cfg.mfact.clamp(0.05, 0.95),
            nmaster: cfg.nmaster,
            screen,
            window_area,
            gaps: cfg.gaps,
            enable_gaps: true,
            smart_gaps: cfg.smart_gaps,
            show_bar: cfg.show_bar,
            top_bar: cfg.top_bar,
            bar_height: cfg.bar_height,
            border_px: cfg.border_px as i32,
            layout: cfg.default_layout,
            prev_layout: cfg.default_layout,
            tagset: [TagMask::bit(0), TagMask::bit(0)],
            seltags: 0,
            clients: Vec::new(),
            stack: Vec::new(),
            sel: None,
            next_id: 1,
            default_gaps: cfg.gaps,
            tag_labels: cfg.tags.clone(),
            scratchpads: cfg.scratchpads.clone(),
            status_text: "dwm-wayland".into(),
            layout_symbol: cfg.default_layout.symbol().to_string(),
        }
    }

    pub fn new(screen: Rect) -> Self {
        Self::from_config(&Config::default(), screen)
    }

    pub fn tagset(&self) -> TagMask {
        self.tagset[self.seltags]
    }

    /// OR `tags` into the active view (used when claiming a newly spawned scratchpad).
    pub fn tagset_mut_union(&mut self, tags: TagMask) {
        let cur = self.tagset[self.seltags];
        self.tagset[self.seltags] = cur.union(tags);
    }

    pub fn set_screen(&mut self, screen: Rect) {
        self.screen = screen;
        self.recompute_window_area();
    }

    pub fn recompute_window_area(&mut self) {
        let bar_h = if self.show_bar { self.bar_height } else { 0 };
        self.window_area = if self.top_bar {
            Rect::new(
                self.screen.x,
                self.screen.y + bar_h,
                self.screen.w,
                self.screen.h - bar_h,
            )
        } else {
            Rect::new(
                self.screen.x,
                self.screen.y,
                self.screen.w,
                self.screen.h - bar_h,
            )
        };
    }

    pub fn manage(&mut self, mut client: Client) -> ClientId {
        if client.id.0 == 0 {
            client.id = ClientId(self.next_id);
            self.next_id += 1;
        } else {
            self.next_id = self.next_id.max(client.id.0 + 1);
        }
        client.border = self.border_px;
        if client.tags.is_empty() {
            // New clients inherit the current workspace view, never scratch tags.
            client.tags = self.tagset().difference(TagMask::SCRATCH_MASK);
            if client.tags.is_empty() {
                client.tags = TagMask::bit(0);
            }
        }
        let id = client.id;
        self.clients.push(client);
        self.attach_stack(id);
        self.sel = Some(id);
        id
    }

    /// If title/app_id matches a scratchpad, retag + float + center it.
    /// Returns `true` when a scratchpad rule applied.
    pub fn apply_scratchpad_rule(&mut self, id: ClientId, title: &str, app_id: &str) -> bool {
        let title_l = title.to_lowercase();
        let app_l = app_id.to_lowercase();
        let Some((index, sp)) = self.scratchpads.iter().enumerate().find(|(_, sp)| {
            let key = sp.match_title.to_lowercase();
            title_l.contains(&key) || app_l.contains(&key)
        }) else {
            return false;
        };
        let tags = TagMask::scratch(index as u32);
        let fw = sp.width;
        let fh = sp.height;
        let fx = self.window_area.x + (self.window_area.w - fw).max(0) / 2;
        let fy = self.window_area.y + (self.window_area.h - fh).max(0) / 2;
        if let Some(c) = self.client_mut(id) {
            c.tags = tags;
            c.floating = true;
            c.float_geom = Rect::new(fx, fy, fw, fh);
            c.geom = c.float_geom;
            if !title.is_empty() {
                c.name = title.to_string();
            }
            if !app_id.is_empty() {
                c.app_id = app_id.to_string();
            }
        }
        // Ensure the scratch tag is visible so the pad shows up immediately.
        let cur = self.tagset[self.seltags];
        self.tagset[self.seltags] = cur.union(tags);
        self.focus(Some(id));
        true
    }

    /// Classic `togglescratch`: XOR the scratch tag into the view; spawn if missing.
    pub fn toggle_scratch(&mut self, index: usize) -> Option<Vec<String>> {
        if index >= self.scratchpads.len() {
            return None;
        }
        let scratchtag = TagMask::scratch(index as u32);
        let existing = self.clients.iter().find(|c| c.tags.contains(scratchtag)).map(|c| c.id);

        if let Some(id) = existing {
            let newtagset = self.tagset().toggle(scratchtag);
            if !newtagset.is_empty() {
                self.tagset[self.seltags] = newtagset;
            }
            if self
                .client(id)
                .map(|c| c.is_visible(self.tagset()))
                .unwrap_or(false)
            {
                self.focus(Some(id));
            } else {
                self.focus_first_visible();
            }
            None
        } else {
            self.tagset[self.seltags] = self.tagset().union(scratchtag);
            Some(self.scratchpads[index].command.clone())
        }
    }

    pub fn set_status(&mut self, text: impl Into<String>) {
        self.status_text = text.into();
    }

    pub fn bar_model(&self) -> BarModel {
        let occ = self.occupied_tags();
        let sel = self.tagset();
        let mut tags = Vec::new();
        for (i, label) in self.tag_labels.iter().enumerate() {
            let bit = TagMask::bit(i as u32);
            let occupied = occ.contains(bit);
            let selected = sel.contains(bit);
            // hide vacant tags (classic patch)
            if !occupied && !selected {
                continue;
            }
            tags.push(BarTag {
                label: label.clone(),
                selected,
                occupied,
                urgent: self
                    .clients
                    .iter()
                    .any(|c| c.urgent && c.tags.contains(bit)),
            });
        }
        let title = self
            .sel
            .and_then(|id| self.client(id))
            .map(|c| c.name.clone())
            .unwrap_or_default();
        BarModel {
            tags,
            layout_symbol: self.layout_symbol.clone(),
            title,
            status: self.status_text.clone(),
            height: if self.show_bar { self.bar_height } else { 0 },
            width: self.screen.w,
            top: self.top_bar,
        }
    }

    pub fn unmanage(&mut self, id: ClientId) {
        self.clients.retain(|c| c.id != id);
        self.stack.retain(|c| *c != id);
        if self.sel == Some(id) {
            self.sel = self.stack.first().copied();
        }
    }

    pub fn client(&self, id: ClientId) -> Option<&Client> {
        self.clients.iter().find(|c| c.id == id)
    }

    pub fn client_mut(&mut self, id: ClientId) -> Option<&mut Client> {
        self.clients.iter_mut().find(|c| c.id == id)
    }

    pub fn visible_clients(&self) -> Vec<&Client> {
        let tags = self.tagset();
        self.clients
            .iter()
            .filter(|c| c.is_visible(tags))
            .collect()
    }

    pub fn tiled_visible(&self) -> Vec<&Client> {
        let tags = self.tagset();
        self.clients
            .iter()
            .filter(|c| c.is_visible(tags) && !c.floating && !c.fullscreen)
            .collect()
    }

    fn attach_stack(&mut self, id: ClientId) {
        self.stack.retain(|c| *c != id);
        self.stack.insert(0, id);
    }

    pub fn focus(&mut self, id: Option<ClientId>) {
        if let Some(id) = id {
            if self.client(id).is_some() {
                self.attach_stack(id);
                self.sel = Some(id);
            }
        } else {
            self.sel = None;
        }
    }

    pub fn focus_stack(&mut self, delta: i32) {
        // Cycle in client-list order (classic dwm `focusstack`), not focus-stack
        // order — otherwise focusing would reshuffle the cycle each step.
        let tags = self.tagset();
        let visible: Vec<ClientId> = self
            .clients
            .iter()
            .filter(|c| c.is_visible(tags) && !c.fullscreen)
            .map(|c| c.id)
            .collect();
        if visible.is_empty() {
            self.sel = None;
            return;
        }
        let cur = self
            .sel
            .and_then(|s| visible.iter().position(|&id| id == s))
            .unwrap_or(0);
        let n = visible.len() as i32;
        let next = ((cur as i32 + delta).rem_euclid(n)) as usize;
        self.focus(Some(visible[next]));
    }

    pub fn push_stack(&mut self, delta: i32) {
        let Some(sel) = self.sel else { return };
        let tags = self.tagset();
        let mut order: Vec<ClientId> = self
            .clients
            .iter()
            .filter(|c| c.is_visible(tags))
            .map(|c| c.id)
            .collect();
        let Some(pos) = order.iter().position(|&id| id == sel) else {
            return;
        };
        if delta == 0 {
            // Zoom: move focused to master (front).
            let id = order.remove(pos);
            order.insert(0, id);
        } else {
            let n = order.len() as i32;
            let new_pos = (pos as i32 + delta).rem_euclid(n) as usize;
            order.swap(pos, new_pos);
        }
        // Reorder clients vec to match.
        let mut reordered = Vec::with_capacity(self.clients.len());
        for id in &order {
            if let Some(c) = self.clients.iter().find(|c| c.id == *id) {
                reordered.push(c.clone());
            }
        }
        for c in &self.clients {
            if !order.contains(&c.id) {
                reordered.push(c.clone());
            }
        }
        self.clients = reordered;
    }

    pub fn view(&mut self, tags: TagMask) {
        if tags.is_empty() {
            // Toggle previous tagset (mod+Tab behaviour).
            self.seltags ^= 1;
        } else if self.tagset[self.seltags] != tags {
            self.seltags ^= 1;
            self.tagset[self.seltags] = tags;
        }
        self.focus_first_visible();
    }

    pub fn toggle_view(&mut self, tags: TagMask) {
        let cur = self.tagset[self.seltags];
        let next = cur.toggle(tags);
        if !next.is_empty() {
            self.tagset[self.seltags] = next;
        }
        self.focus_first_visible();
    }

    pub fn tag_selected(&mut self, tags: TagMask) {
        if tags.is_empty() {
            return;
        }
        if let Some(sel) = self.sel {
            if let Some(c) = self.client_mut(sel) {
                c.tags = tags;
            }
        }
        self.focus_first_visible();
    }

    pub fn toggle_tag_selected(&mut self, tags: TagMask) {
        if let Some(sel) = self.sel {
            if let Some(c) = self.client_mut(sel) {
                let next = c.tags.toggle(tags);
                if !next.is_empty() {
                    c.tags = next;
                }
            }
        }
    }

    pub fn set_layout(&mut self, kind: LayoutKind) {
        if self.layout == kind {
            std::mem::swap(&mut self.layout, &mut self.prev_layout);
        } else {
            self.prev_layout = self.layout;
            self.layout = kind;
        }
    }

    pub fn cycle_layout(&mut self) {
        self.prev_layout = self.layout;
        self.layout = self.layout.next();
    }

    pub fn inc_nmaster(&mut self, delta: i32) {
        let next = self.nmaster as i32 + delta;
        self.nmaster = next.max(0) as usize;
    }

    pub fn set_mfact(&mut self, delta: f32) {
        self.mfact = (self.mfact + delta).clamp(0.05, 0.95);
    }

    pub fn toggle_floating(&mut self) {
        if let Some(sel) = self.sel {
            if let Some(c) = self.client_mut(sel) {
                c.floating = !c.floating;
            }
        }
    }

    pub fn toggle_sticky(&mut self) {
        if let Some(sel) = self.sel {
            if let Some(c) = self.client_mut(sel) {
                c.sticky = !c.sticky;
            }
        }
    }

    pub fn toggle_gaps(&mut self) {
        self.enable_gaps = !self.enable_gaps;
    }

    pub fn toggle_smart_gaps(&mut self) {
        self.smart_gaps = !self.smart_gaps;
    }

    pub fn default_gaps(&mut self) {
        self.gaps = self.default_gaps;
        self.enable_gaps = true;
    }

    pub fn inc_gaps(&mut self, delta: i32) {
        let add = |v: u32| -> u32 { (v as i32 + delta).max(0) as u32 };
        self.gaps.inner_h = add(self.gaps.inner_h);
        self.gaps.inner_v = add(self.gaps.inner_v);
        self.gaps.outer_h = add(self.gaps.outer_h);
        self.gaps.outer_v = add(self.gaps.outer_v);
    }

    /// Cycle view to the next/previous occupied tag (shiftview patch).
    pub fn shift_view(&mut self, delta: i32) {
        if delta == 0 {
            return;
        }
        let mut occupied = TagMask::EMPTY;
        for c in &self.clients {
            occupied = occupied.union(c.tags.intersect(TagMask::ALL_WORKSPACE));
        }
        let mut next = self.tagset().rotate(delta);
        let mut guard = 0;
        while !occupied.is_empty() && !next.contains(occupied) && guard < NUM_TAGS {
            next = next.rotate(delta);
            guard += 1;
        }
        self.view(next);
    }

    fn focus_first_visible(&mut self) {
        let tags = self.tagset();
        let id = self
            .stack
            .iter()
            .copied()
            .find(|id| self.client(*id).map(|c| c.is_visible(tags)).unwrap_or(false));
        self.sel = id;
    }

    /// Run the active layout and update client geometries.
    pub fn arrange(&mut self) -> (Vec<(ClientId, Rect)>, String) {
        self.recompute_window_area();
        let tags = self.tagset();
        let params = ArrangeParams {
            window_area: self.window_area,
            mfact: self.mfact,
            nmaster: self.nmaster,
            gaps: self.gaps,
            enable_gaps: self.enable_gaps,
            smart_gaps: self.smart_gaps,
            border_px: self.border_px,
            layout: self.layout,
        };

        // Collect tiled ids first to avoid borrow issues.
        let tiled_ids: Vec<ClientId> = self
            .clients
            .iter()
            .filter(|c| c.is_visible(tags) && !c.floating && !c.fullscreen)
            .map(|c| c.id)
            .collect();

        let tiled_refs: Vec<&Client> = tiled_ids
            .iter()
            .filter_map(|id| self.client(*id))
            .collect();

        let (rects, symbol) = layout::arrange(&tiled_refs, params);
        self.layout_symbol = symbol.clone();

        let mut geometries = Vec::new();
        for (id, rect) in tiled_ids.into_iter().zip(rects.into_iter()) {
            if let Some(c) = self.client_mut(id) {
                c.geom = rect;
            }
            geometries.push((id, rect));
        }

        // Floating / fullscreen keep their own geometry but are still reported.
        let floating: Vec<(ClientId, Rect)> = self
            .clients
            .iter()
            .filter(|c| c.is_visible(tags) && (c.floating || c.fullscreen))
            .map(|c| {
                let geom = if c.fullscreen {
                    self.screen
                } else {
                    c.float_geom
                };
                (c.id, geom)
            })
            .collect();
        for (id, geom) in floating {
            if let Some(c) = self.client_mut(id) {
                c.geom = geom;
            }
            geometries.push((id, geom));
        }

        // Hide non-visible by reporting zero-size offscreen (compositor may unmap).
        for c in &self.clients {
            if !c.is_visible(tags) {
                geometries.push((c.id, Rect::new(-c.geom.w, -c.geom.h, c.geom.w, c.geom.h)));
            }
        }

        (geometries, symbol)
    }

    /// Tags that currently have at least one client (for hide-vacant-tags).
    pub fn occupied_tags(&self) -> TagMask {
        let mut mask = TagMask::EMPTY;
        for c in &self.clients {
            mask = mask.union(c.tags.intersect(TagMask::ALL_WORKSPACE));
        }
        mask
    }
}
