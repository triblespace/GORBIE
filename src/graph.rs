//! One force-directed layout and one drawing kit, shared by every graph view.
//!
//! # Why a simulation, and why it is allowed to move
//!
//! Both graph widgets in this crate used to open with a paragraph explaining
//! why they were *not* force-directed. The mesh argued that "a force-directed
//! layout earns its cost on hundreds of nodes with latent structure", and a
//! peer mesh is a handful of nodes; the lattice argued that a partial order
//! already has a canonical picture and a simulation would discard it. Both
//! arguments were right about the sizes they were written for and both are
//! answered by the sizes these views now have to reach: a thousand-peer colony
//! *is* hundreds of nodes with latent structure, and the derivation chain the
//! lattice is asked to draw all at once is tens of thousands.
//!
//! The mesh's sharper objection survives and is worth answering directly: "the
//! same report renders differently twice, which is exactly what an instrument
//! must not do." The answer is not that determinism stopped mattering. It is
//! that **an instrument may show motion that means something**. A settled field
//! with one shimmering chain in it is telling you where the change landed, and
//! it is telling you in a channel that costs no ink, no colour and no legend.
//! What an instrument must not do is move *without meaning*, and that is why
//! energy here is rationed as tightly as saturation: see [`ForceLayout::heat`].
//!
//! # Where the physics came from
//!
//! [`layout`] is a lift, not a rewrite. The force law, its constants and its
//! two momentum corrections are the ones the wiki viewer ran on the GPU, and
//! [`LayoutParams::default`] is that law constant for constant.
//!
//! # The seam
//!
//! **The layout owns positions by index; the view owns everything else at the
//! same index.** There is no generic node type and no payload. A view keeps its
//! own index-parallel arrays — labels, glyphs, presence, coverage — and reads
//! [`ForceLayout::positions`] beside them. That is the entire contract, and it
//! is what lets three views with nothing in common share one solver.

use std::collections::BTreeMap;
use std::sync::Arc;

use eframe::egui::{Id, Ui};
use parking_lot::Mutex;

pub mod layout;
pub mod marks;
mod tree;
pub mod viewport;

pub use layout::{Anchors, ForceLayout, LayoutParams, LayoutStats, RetargetReport};
pub use marks::{
    arc_points, draw_link, draw_node, draw_track, Glyph, Stroke2, CLEARANCE, HIT, MARK, TRACK,
};
pub use viewport::{pick, GraphViewport, Lod};

/// A layout plus the caller's node keys, so a changed graph can be *retargeted*
/// instead of rebuilt.
///
/// Identity lives here and not in [`ForceLayout`], because which new node is
/// which old node is a question about a view's own vocabulary. A key is a UI
/// handle and nothing else: it is never used to look anything up in a store,
/// and two nodes that collide on one simply lose their carried position, which
/// costs a settle and not a correctness property.
pub struct KeyedLayout {
    layout: ForceLayout,
    keys: Vec<u64>,
    generation: u64,
}

impl KeyedLayout {
    pub fn new(keys: Vec<u64>, edges: &[(u32, u32)], params: LayoutParams) -> Self {
        KeyedLayout {
            layout: ForceLayout::new(keys.len(), edges, params),
            keys,
            generation: 0,
        }
    }

    /// Bumped whenever the topology changes.
    ///
    /// A view that caches anything derived from the edge set — a chain, a
    /// ranking, a lane assignment — compares this instead of recomputing per
    /// frame. The lattice's chain walk was `O(n * e)` *every frame*, which is
    /// 6.4e8 operations on a real derivation chain, to answer a question that
    /// only changes when somebody clicks.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Bring the layout up to date with `keys` and `edges`.
    ///
    /// Returns `None` when nothing changed — the common case, once per frame.
    /// When something did change, survivors keep their position *and* their
    /// velocity and the newcomers are heated, so one write by one peer nudges
    /// the picture instead of resetting it. The viewer this was lifted from did
    /// the opposite: it dropped the whole graph on every dataset revision, so
    /// any `wiki create` by any zooid teleported the layout back to a circle.
    pub fn sync(&mut self, keys: &[u64], edges: &[(u32, u32)]) -> Option<RetargetReport> {
        if self.keys == keys {
            return None;
        }
        let mut previous: BTreeMap<u64, u32> = BTreeMap::new();
        for (index, key) in self.keys.iter().enumerate() {
            previous.entry(*key).or_insert(index as u32);
        }
        let carry: Vec<Option<u32>> = keys.iter().map(|key| previous.get(key).copied()).collect();
        let report = self.layout.retarget(keys.len(), edges, &carry);
        self.generation += 1;
        self.keys.clear();
        self.keys.extend_from_slice(keys);
        Some(report)
    }

    pub fn layout(&self) -> &ForceLayout {
        &self.layout
    }

    pub fn layout_mut(&mut self) -> &mut ForceLayout {
        &mut self.layout
    }

    pub fn keys(&self) -> &[u64] {
        &self.keys
    }

    /// Index of a key, for a view that needs to find its own node again.
    pub fn index_of(&self, key: u64) -> Option<usize> {
        self.keys.iter().position(|other| *other == key)
    }
}

/// The layout attached to `id`, created on first sight and retargeted after.
///
/// Held behind an `Arc` in egui's temporary data so a redraw costs a pointer
/// clone rather than a copy of every position — which at the sizes the chain
/// view reaches would be the most expensive thing in the frame.
pub fn shared(
    ui: &Ui,
    id: Id,
    keys: &[u64],
    edges: &[(u32, u32)],
    params: LayoutParams,
) -> (Arc<Mutex<KeyedLayout>>, bool) {
    let mut created = false;
    let shared = ui.ctx().data_mut(|data| {
        data.get_temp_mut_or_insert_with(id, || {
            created = true;
            Arc::new(Mutex::new(KeyedLayout::new(keys.to_vec(), edges, params)))
        })
        .clone()
    });
    let changed = created || shared.lock().sync(keys, edges).is_some();
    (shared, changed)
}

/// A stable UI key for a byte handle.
///
/// This mixes bytes for a hash map keyed by screen identity. It is **not** a
/// derived id and nothing is ever looked up by it in a store; a collision costs
/// one node its carried position.
pub fn key_of(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x1000_0000_01b3);
    }
    hash
}
