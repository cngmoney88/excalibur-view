//! The shapes of a model, as triangles, for drawing it.
//!
//! A steel model's parts are mostly extrusions with holes and copes cut out
//! of them, and cutting is the slow part: a ladder rail with fifteen rung
//! holes takes ifc-lite ten seconds or more on its own. So a model is drawn
//! in two passes. The first draws every part as it is before it is cut, which
//! takes a fraction of a second for a whole building. The second makes each
//! part that has cuts properly, on as many threads as the computer can spare,
//! and each one replaces its uncut shape as it is done. The quickest go first,
//! so the model settles part by part and the few slow ones come last.
//!
//! Positions are metres, relative to each shape's own `origin`, so a model
//! placed a long way from its survey point keeps its precision.

use std::cell::Cell;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc};

use ifc_lite_core::{
    build_entity_index, has_geometry_by_name, DecodedEntity, EntityDecoder, EntityIndex, EntityScanner,
    IfcSchema, IfcType,
};
use ifc_lite_geometry::{GeometryProcessor, GeometryRouter, Mesh, TessellationQuality};

/// Cutting recurses once per cut; a part with hundreds of holes needs room.
const STACK: usize = 32 * 1024 * 1024;

/// What a shape is, for colouring it and for showing or hiding a kind at once.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Group {
    Column,
    Beam,
    Brace,
    Plate,
    Bolt,
    Concrete,
    Other,
}

impl Group {
    pub const ALL: [Group; 7] =
        [Group::Column, Group::Beam, Group::Brace, Group::Plate, Group::Bolt, Group::Concrete, Group::Other];

    pub fn name(self) -> &'static str {
        match self {
            Group::Column => "Columns",
            Group::Beam => "Beams",
            Group::Brace => "Braces and members",
            Group::Plate => "Plates",
            Group::Bolt => "Bolts",
            Group::Concrete => "Slabs, walls, footings",
            Group::Other => "Everything else",
        }
    }

    /// The group an entity type draws in, or `None` for one that isn't drawn:
    /// no shape of its own, or a shape that isn't a thing (an opening, a room,
    /// the site, a grid).
    pub fn of(entity: &str) -> Option<Group> {
        let upper = entity.to_ascii_uppercase();
        if !has_geometry_by_name(&upper) {
            return None;
        }
        Some(match upper.as_str() {
            "IFCOPENINGELEMENT" | "IFCOPENINGSTANDARDCASE" | "IFCVOIDINGFEATURE" | "IFCSPACE" | "IFCSITE"
            | "IFCBUILDING" | "IFCBUILDINGSTOREY" | "IFCANNOTATION" | "IFCGRID" | "IFCVIRTUALELEMENT"
            | "IFCELEMENTASSEMBLY" | "IFCSPATIALZONE" | "IFCEXTERNALSPATIALELEMENT" => return None,
            t if t.starts_with("IFCCOLUMN") => Group::Column,
            t if t.starts_with("IFCBEAM") => Group::Beam,
            t if t.starts_with("IFCMEMBER") => Group::Brace,
            t if t.starts_with("IFCPLATE") => Group::Plate,
            "IFCMECHANICALFASTENER" | "IFCFASTENER" => Group::Bolt,
            t if t.starts_with("IFCSLAB")
                || t.starts_with("IFCWALL")
                || t.starts_with("IFCFOOTING")
                || t.starts_with("IFCPILE")
                || t.starts_with("IFCREINFORC") =>
            {
                Group::Concrete
            }
            _ => Group::Other,
        })
    }
}

/// One thing's triangles.
pub struct Shape {
    /// The entity's number in the file, `#81`: the same as `Part::id`.
    pub id: u32,
    pub group: Group,
    /// What the model calls it.
    pub name: String,
    /// Metres. Each position is relative to this.
    pub origin: [f64; 3],
    pub positions: Vec<f32>,
    pub normals: Vec<f32>,
    pub indices: Vec<u32>,
    /// False for a shape drawn before its holes and copes are cut; the cut
    /// one follows as `News::Cut`.
    pub exact: bool,
}

/// What the drawing threads have to say, in the order they say it.
pub enum News {
    /// Every shape in the model, quickly, some of them not yet cut.
    Rough {
        shapes: Vec<Shape>,
        /// Things that couldn't be drawn at all, and why.
        failed: Vec<(u32, String)>,
        /// How many shapes are still to be cut.
        to_cut: usize,
    },
    /// One shape with its cuts made, in place of its rough one.
    Cut(Shape),
    /// A shape whose cuts couldn't be made. Its rough one stays.
    Uncuttable(u32, String),
    /// All done.
    Finished,
}

/// A model being drawn. Dropping it stops the threads after the part each
/// is on.
pub struct Meshing {
    pub news: mpsc::Receiver<News>,
    stop: Arc<AtomicBool>,
}

impl Drop for Meshing {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// Starts drawing a model: one pass for every shape, then `threads` threads
/// cutting.
pub fn start(bytes: Arc<Vec<u8>>, threads: usize) -> Meshing {
    let (tx, rx) = mpsc::channel();
    let stop = Arc::new(AtomicBool::new(false));
    let stopping = stop.clone();
    // If the thread can't be started, the sender is dropped with it and
    // the receiver says so.
    let _ = std::thread::Builder::new()
        .name("drawing a model".into())
        .stack_size(STACK)
        .spawn(move || run(bytes, threads.max(1), tx, stopping));
    Meshing { news: rx, stop }
}

fn run(bytes: Arc<Vec<u8>>, threads: usize, tx: mpsc::Sender<News>, stop: Arc<AtomicBool>) {
    let index = Arc::new(build_entity_index(bytes.as_slice()));
    let (things, project) = drawable(&bytes);
    let scale = {
        let mut decoder = EntityDecoder::with_arc_index(bytes.as_slice(), index.clone());
        project
            .and_then(|id| ifc_lite_core::extract_length_unit_scale(&mut decoder, id).ok())
            .unwrap_or(1.0)
    };

    let mut shapes = Vec::with_capacity(things.len());
    let mut failed = Vec::new();
    let mut to_cut: Vec<(u32, Group, usize)> = Vec::new();
    {
        let mut mesher = Mesher::new(&bytes, &index, scale, true);
        for &(id, group) in &things {
            if stop.load(Ordering::Relaxed) {
                return;
            }
            mesher.cuts.set(0);
            match mesher.make(id, group, false) {
                Ok(Some(shape)) => shapes.push(shape),
                Ok(None) => {}
                Err(why) => failed.push((id, why)),
            }
            if mesher.cuts.get() > 0 {
                to_cut.push((id, group, mesher.cuts.get()));
            }
        }
    }
    let count = to_cut.len();
    if tx.send(News::Rough { shapes, failed, to_cut: count }).is_err() {
        return;
    }

    // Fewest cuts first: most parts settle at once, the ladder rails last.
    to_cut.sort_by_key(|&(id, _, cuts)| (cuts, id));
    let jobs = Arc::new(to_cut);
    let next = Arc::new(AtomicUsize::new(0));
    // Tekla writes the same connection plate or the same cut beam end over
    // and over; one cache between the threads makes each of those once.
    let seen = GeometryRouter::new_dedup_cache();
    let workers: Vec<_> = (0..threads.min(count))
        .filter_map(|n| {
            let (bytes, index, jobs, next, tx, stop, seen) = (
                bytes.clone(),
                index.clone(),
                jobs.clone(),
                next.clone(),
                tx.clone(),
                stop.clone(),
                seen.clone(),
            );
            std::thread::Builder::new()
                .name(format!("cutting a model {n}"))
                .stack_size(STACK)
                .spawn(move || {
                    let mut mesher = Mesher::new(&bytes, &index, scale, false);
                    mesher.router.enable_content_dedup_shared(seen);
                    while !stop.load(Ordering::Relaxed) {
                        let Some(&(id, group, _)) = jobs.get(next.fetch_add(1, Ordering::Relaxed)) else {
                            return;
                        };
                        let news = match mesher.make(id, group, true) {
                            Ok(Some(shape)) => News::Cut(shape),
                            Ok(None) => News::Uncuttable(id, "nothing is left of it once it is cut".into()),
                            Err(why) => News::Uncuttable(id, why),
                        };
                        if tx.send(news).is_err() {
                            return;
                        }
                    }
                })
                .ok()
        })
        .collect();
    for worker in workers {
        let _ = worker.join();
    }
    let _ = tx.send(News::Finished);
}

/// Everything in the file that draws, and the project, for its units.
fn drawable(bytes: &[u8]) -> (Vec<(u32, Group)>, Option<u32>) {
    let mut things = Vec::new();
    let mut project = None;
    let mut scanner = EntityScanner::new(bytes);
    while let Some((id, entity, _, _)) = scanner.next_entity() {
        if project.is_none() && entity.eq_ignore_ascii_case("IFCPROJECT") {
            project = Some(id);
        } else if let Some(group) = Group::of(entity) {
            things.push((id, group));
        }
    }
    (things, project)
}

/// One thread's decoder and geometry router.
struct Mesher<'a> {
    bytes: &'a [u8],
    index: Arc<EntityIndex>,
    scale: f64,
    rough: bool,
    decoder: EntityDecoder<'a>,
    router: GeometryRouter,
    /// Cuts the rough pass left out of the last part it drew.
    cuts: Rc<Cell<usize>>,
}

impl<'a> Mesher<'a> {
    fn new(bytes: &'a [u8], index: &Arc<EntityIndex>, scale: f64, rough: bool) -> Mesher<'a> {
        let cuts = Rc::new(Cell::new(0));
        Mesher {
            bytes,
            index: index.clone(),
            scale,
            rough,
            decoder: EntityDecoder::with_arc_index(bytes, index.clone()),
            router: router_for(scale, rough, &cuts),
            cuts,
        }
    }

    fn make(&mut self, id: u32, group: Group, exact: bool) -> Result<Option<Shape>, String> {
        let entity = self.decoder.decode_by_id(id).map_err(|e| e.to_string())?;
        let (router, decoder) = (&self.router, &mut self.decoder);
        let made = catch_unwind(AssertUnwindSafe(|| router.process_element(&entity, decoder)));
        let mesh = match made {
            Ok(Ok(mesh)) => mesh,
            Ok(Err(e)) => return Err(e.to_string()),
            Err(_) => {
                // Whatever it was part-way through is suspect now: start
                // this thread's decoder and router afresh.
                self.decoder = EntityDecoder::with_arc_index(self.bytes, self.index.clone());
                self.router = router_for(self.scale, self.rough, &self.cuts);
                return Err("the geometry library gave up on it".into());
            }
        };
        if mesh.indices.is_empty() || mesh.positions.is_empty() {
            return Ok(None);
        }
        Ok(Some(Shape {
            id,
            group,
            name: entity.get_string(2).map(|s| s.trim().to_string()).unwrap_or_default(),
            origin: mesh.origin,
            positions: mesh.positions,
            normals: mesh.normals,
            indices: mesh.indices,
            exact,
        }))
    }
}

fn router_for(scale: f64, rough: bool, cuts: &Rc<Cell<usize>>) -> GeometryRouter {
    let mut router = GeometryRouter::with_scale_and_local_frame(scale, true);
    if rough {
        // The stand-in meshes in the file's own units, as the processor it
        // replaces does; the element's router scales the result with the rest.
        router.register(Box::new(Uncut { inner: GeometryRouter::new(), cuts: cuts.clone() }));
    }
    router
}

/// The rough pass's stand-in for ifc-lite's boolean processor: the solid
/// before anything is cut from it, and a count of the cuts left out.
struct Uncut {
    inner: GeometryRouter,
    cuts: Rc<Cell<usize>>,
}

impl GeometryProcessor for Uncut {
    fn process(
        &self,
        entity: &DecodedEntity,
        decoder: &mut EntityDecoder,
        _schema: &IfcSchema,
        _quality: TessellationQuality,
    ) -> ifc_lite_geometry::Result<Mesh> {
        let mut solid = entity.clone();
        let mut cuts = 0;
        while matches!(solid.ifc_type, IfcType::IfcBooleanResult | IfcType::IfcBooleanClippingResult) && cuts < 4096 {
            let first = solid
                .get_ref(1)
                .ok_or_else(|| ifc_lite_geometry::Error::geometry(String::from("a cut with nothing to cut")))?;
            solid = decoder.decode_by_id(first)?;
            cuts += 1;
        }
        self.cuts.set(self.cuts.get() + cuts);
        self.inner.process_representation_item(&solid, decoder)
    }

    fn supported_types(&self) -> Vec<IfcType> {
        vec![IfcType::IfcBooleanResult, IfcType::IfcBooleanClippingResult]
    }
}

/// The exact shapes of the things named, in order, on this thread. One whose
/// shape can't be made is left out and named with the reason, rather than
/// drawn wrong.
pub fn of(bytes: &[u8], ids: &[u32]) -> (Vec<Shape>, Vec<(u32, String)>) {
    let index = Arc::new(build_entity_index(bytes));
    let (things, project) = drawable(bytes);
    let scale = {
        let mut decoder = EntityDecoder::with_arc_index(bytes, index.clone());
        project
            .and_then(|id| ifc_lite_core::extract_length_unit_scale(&mut decoder, id).ok())
            .unwrap_or(1.0)
    };
    let group_of: std::collections::HashMap<u32, Group> = things.into_iter().collect();
    let mut mesher = Mesher::new(bytes, &index, scale, false);
    let mut shapes = Vec::with_capacity(ids.len());
    let mut failed = Vec::new();
    for &id in ids {
        let group = group_of.get(&id).copied().unwrap_or(Group::Other);
        match mesher.make(id, group, true) {
            Ok(Some(shape)) => shapes.push(shape),
            Ok(None) => failed.push((id, "it has no shape".into())),
            Err(why) => failed.push((id, why)),
        }
    }
    (shapes, failed)
}
