use std::fs;
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use ed25519_dalek::SigningKey;
use rand_core06::OsRng;
use tracing::Subscriber;
use tracing_subscriber::layer::{Context, Layer};
use tracing_subscriber::prelude::*;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::EnvFilter;
use triblespace::core::metadata;
use triblespace::core::repo::pile::Pile;
use triblespace::core::repo::Repository;
use triblespace::prelude::blobschemas::LongString;
use triblespace::prelude::valueschemas::{Blake3, GenId, Handle, ShortString, U256BE};
use triblespace::prelude::*;

// Telemetry is intentionally its own pile to keep profiling noise out of any
// "real" data model piles. This module installs a tracing subscriber that
// converts spans into tribles and commits them in batches on a background thread.

pub mod schema {
    use super::*;

    attributes! {
        "FCFE9BFBC865C48001535A15594FB4D6" as pub kind: GenId;
        "3E062AA7E3554C8F2DB94883CE639BFE" as pub session: GenId;
        "146E5AA2F7CB3D8B654BC7742A13CAB3" as pub parent: GenId;
        "CCB0147D20C4C6FCAC0E3D87FAFF71D1" as pub name: Handle<Blake3, LongString>;
        "8A4BE2C4D0E90D2B9EE0E1A07ECA2CFA" as pub category: ShortString;
        "E11A84A30CC112650DC860B66B8BD8A9" as pub begin_ns: U256BE;
        "2786FA563372FB6EF469EC7710719A49" as pub end_ns: U256BE;
        "7593602383D0B0D21BBE382A67E5BD9F" as pub duration_ns: U256BE;
        "835737CC7A2E4449B8F413CDD753EE6B" as pub card_index: U256BE;
        "7E96DD9A0B5002796B645ED25F5E99AC" as pub source: Handle<Blake3, LongString>;
    }

    #[allow(non_upper_case_globals)]
    pub const kind_session: Id = triblespace::macros::id_hex!("2701F7019B865D461F0169B1303026D6");
    #[allow(non_upper_case_globals)]
    pub const kind_span: Id = triblespace::macros::id_hex!("0AF9FEB9A2BFEB1BE8A8229829181085");

    #[allow(non_upper_case_globals)]
    pub const telemetry_metadata: Id =
        triblespace::macros::id_hex!("BCFDE38F7E452924C72803239392EA05");

    pub fn build_telemetry_metadata<B>(blobs: &mut B) -> std::result::Result<TribleSet, B::PutError>
    where
        B: BlobStore<Blake3>,
    {
        let mut metadata_set = TribleSet::new();

        metadata_set += entity! { ExclusiveId::force_ref(&telemetry_metadata) @
            metadata::shortname: "gorbie_telemetry",
            metadata::description: blobs.put::<LongString, _>(
                "Span-based profiling events emitted by GORBIE when the `telemetry` feature is enabled.",
            )?,
        };

        metadata_set.union(<GenId as metadata::ConstMetadata>::describe(blobs)?);
        metadata_set.union(<ShortString as metadata::ConstMetadata>::describe(blobs)?);
        metadata_set.union(<U256BE as metadata::ConstMetadata>::describe(blobs)?);
        metadata_set
            .union(<Handle<Blake3, LongString> as metadata::ConstMetadata>::describe(blobs)?);
        metadata_set.union(<LongString as metadata::ConstMetadata>::describe(blobs)?);

        fn describe_kind<B>(
            blobs: &mut B,
            kind_id: &Id,
            shortname: &str,
            description: &str,
        ) -> std::result::Result<TribleSet, B::PutError>
        where
            B: BlobStore<Blake3>,
        {
            Ok(entity! { ExclusiveId::force_ref(kind_id) @
                metadata::shortname: shortname,
                metadata::description: blobs.put::<LongString, _>(description.to_string())?,
            })
        }

        metadata_set.union(describe_kind(
            blobs,
            &kind_session,
            "telemetry_session",
            "A profiling session. Groups spans emitted during one notebook run.",
        )?);
        metadata_set.union(describe_kind(
            blobs,
            &kind_span,
            "telemetry_span",
            "A begin/end span with optional parent links.",
        )?);

        fn describe_attribute<B, S>(
            blobs: &mut B,
            attribute: &Attribute<S>,
            shortname: &str,
        ) -> std::result::Result<TribleSet, B::PutError>
        where
            B: BlobStore<Blake3>,
            S: ValueSchema,
        {
            let mut tribles = metadata::Metadata::describe(attribute, blobs)?;
            let attribute_id = metadata::Metadata::id(attribute);
            tribles += entity! { ExclusiveId::force_ref(&attribute_id) @
                metadata::shortname: shortname,
                metadata::description: blobs.put::<LongString, _>(shortname.to_string())?,
            };
            Ok(tribles)
        }

        macro_rules! add_attr {
            ($attr:expr, $name:expr) => {
                metadata_set.union(describe_attribute(blobs, &$attr, $name)?);
            };
        }

        add_attr!(kind, "telemetry_kind");
        add_attr!(session, "telemetry_session");
        add_attr!(parent, "telemetry_parent");
        add_attr!(name, "telemetry_name");
        add_attr!(category, "telemetry_category");
        add_attr!(begin_ns, "telemetry_begin_ns");
        add_attr!(end_ns, "telemetry_end_ns");
        add_attr!(duration_ns, "telemetry_duration_ns");
        add_attr!(card_index, "telemetry_card_index");
        add_attr!(source, "telemetry_source");

        Ok(metadata_set)
    }
}

fn is_valid_short(value: &str) -> bool {
    value.as_bytes().len() <= 32 && !value.as_bytes().iter().any(|b| *b == 0)
}

#[derive(Clone)]
struct TelemetryHandle {
    session: Id,
    base: Instant,
    tx: mpsc::SyncSender<SinkMsg>,
}

impl TelemetryHandle {
    fn now_ns(&self) -> u64 {
        self.base.elapsed().as_nanos() as u64
    }

    fn emit(&self, msg: SinkMsg) {
        // Best-effort: never block the UI thread.
        let _ = self.tx.try_send(msg);
    }
}

#[derive(Debug)]
enum SinkMsg {
    Begin(BeginMsg),
    End(EndMsg),
    Shutdown { end_ns: u64 },
}

#[derive(Debug, Clone)]
struct BeginMsg {
    span: Id,
    parent: Option<Id>,
    session: Id,
    at_ns: u64,
    category: &'static str,
    name: String,
    card_index: Option<u64>,
    source: Option<String>,
}

#[derive(Debug, Clone)]
struct EndMsg {
    span: Id,
    at_ns: u64,
    duration_ns: u64,
}

#[derive(Debug, Clone, Copy)]
struct GorbieSpanData {
    span: Id,
    start_ns: u64,
}

#[derive(Default)]
struct FieldCapture {
    card_index: Option<u64>,
    source: Option<String>,
}

impl tracing::field::Visit for FieldCapture {
    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        match field.name() {
            "card_index" => self.card_index = Some(value),
            _ => {}
        }
    }

    fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
        match field.name() {
            "card_index" if value >= 0 => self.card_index = Some(value as u64),
            _ => {}
        }
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        match field.name() {
            "source" if !value.is_empty() => self.source = Some(value.to_string()),
            _ => {}
        }
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        match field.name() {
            "card_index" => {
                let raw = format!("{value:?}");
                if let Ok(value) = raw.parse::<u64>() {
                    self.card_index = Some(value);
                }
            }
            "source" => {
                let mut raw = format!("{value:?}");
                if raw.starts_with('"') && raw.ends_with('"') && raw.len() >= 2 {
                    raw = raw[1..raw.len() - 1].to_string();
                }
                if !raw.is_empty() {
                    self.source = Some(raw);
                }
            }
            _ => {}
        }
    }
}

/// Tracing layer that turns spans into TribleSpace telemetry.
///
/// Construct via [`Telemetry::layer_from_env`] and attach to your application's subscriber.
pub struct GorbieTelemetryLayer {
    handle: TelemetryHandle,
}

impl GorbieTelemetryLayer {
    fn parent_id<S>(&self, attrs: &tracing::span::Attributes<'_>, ctx: &Context<'_, S>) -> Option<Id>
    where
        S: Subscriber + for<'a> LookupSpan<'a>,
    {
        // Explicit parent beats contextual parent.
        if let Some(parent) = attrs.parent() {
            if let Some(span) = ctx.span(parent) {
                if let Some(data) = span.extensions().get::<GorbieSpanData>() {
                    return Some(data.span);
                }
            }
        }

        if let Some(id) = ctx.current_span().id() {
            if let Some(span) = ctx.span(id) {
                if let Some(data) = span.extensions().get::<GorbieSpanData>() {
                    return Some(data.span);
                }
            }
        }

        None
    }
}

impl<S> Layer<S> for GorbieTelemetryLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, attrs: &tracing::span::Attributes<'_>, id: &tracing::span::Id, ctx: Context<'_, S>) {
        let Some(span) = ctx.span(id) else {
            return;
        };

        let meta = attrs.metadata();
        let mut fields = FieldCapture::default();
        attrs.record(&mut fields);

        let start_ns = self.handle.now_ns();
        let span_id = *ufoid();
        let parent = self.parent_id(attrs, &ctx);

        span.extensions_mut().insert(GorbieSpanData {
            span: span_id,
            start_ns,
        });

        let target = meta.target();
        let category = target.split("::").next().unwrap_or(target);
        let category = if !category.is_empty() && is_valid_short(category) {
            category
        } else {
            "span"
        };
        let name = meta.name().to_string();

        self.handle.emit(SinkMsg::Begin(BeginMsg {
            span: span_id,
            parent,
            session: self.handle.session,
            at_ns: start_ns,
            category,
            name,
            card_index: fields.card_index,
            source: fields.source,
        }));
    }

    fn on_close(&self, id: tracing::span::Id, ctx: Context<'_, S>) {
        let Some(span) = ctx.span(&id) else {
            return;
        };
        let Some(data) = span.extensions().get::<GorbieSpanData>().copied() else {
            return;
        };

        let end_ns = self.handle.now_ns();
        self.handle.emit(SinkMsg::End(EndMsg {
            span: data.span,
            at_ns: end_ns,
            duration_ns: end_ns.saturating_sub(data.start_ns),
        }));
    }
}

pub struct Telemetry {
    base: Instant,
    tx: mpsc::SyncSender<SinkMsg>,
    join: Option<thread::JoinHandle<()>>,
}

impl Telemetry {
    /// Start a telemetry sink and return a layer that writes spans into it.
    ///
    /// This does **not** install a tracing subscriber. Embed the returned layer into your
    /// application's subscriber, and keep the returned [`Telemetry`] guard alive to
    /// flush and close the sink on shutdown.
    pub fn layer_from_env(notebook_title: &str) -> Option<(GorbieTelemetryLayer, Self)> {
        let pile_path = std::env::var("GORBIE_TELEMETRY_PILE").ok()?;
        let pile_path = pile_path.trim();
        if pile_path.is_empty() {
            return None;
        }
        let pile_path = PathBuf::from(pile_path);

        let flush_ms = std::env::var("GORBIE_TELEMETRY_FLUSH_MS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(250);
        let flush_interval = Duration::from_millis(flush_ms.max(10));

        let queue_cap = std::env::var("GORBIE_TELEMETRY_QUEUE")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(4096);

        let base = Instant::now();
        let session_id = *ufoid();

        let (tx, rx) = mpsc::sync_channel(queue_cap.max(64));

        let title = notebook_title.to_string();
        let join = thread::Builder::new()
            .name("gorbie-telemetry".to_string())
            .spawn(move || {
                if let Err(err) = run_sink(pile_path, title, session_id, flush_interval, rx) {
                    log::warn!("gorbie telemetry sink failed: {err}");
                }
            })
            .ok()?;

        let handle = TelemetryHandle {
            session: session_id,
            base,
            tx: tx.clone(),
        };
        let layer = GorbieTelemetryLayer { handle };

        Some((
            layer,
            Self {
                base,
                tx,
                join: Some(join),
            },
        ))
    }

    /// Convenience for standalone notebooks: start telemetry and install a global subscriber
    /// (only if none exists).
    pub fn install_global_from_env(notebook_title: &str) -> Option<Self> {
        let (layer, guard) = Self::layer_from_env(notebook_title)?;

        // Keep default noise low, but ensure GORBIE spans are visible.
        let filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("warn,GORBIE=info"));
        let subscriber = tracing_subscriber::registry().with(filter).with(layer);

        if tracing::subscriber::set_global_default(subscriber).is_err() {
            log::warn!("gorbie telemetry disabled: tracing subscriber already set");
            drop(guard);
            return None;
        }

        Some(guard)
    }
}

impl Drop for Telemetry {
    fn drop(&mut self) {
        // Make a best-effort attempt to flush and close the pile. This is
        // deliberately blocking during shutdown, but it should be bounded since
        // the sink is always draining the channel.
        let _ = self.tx.send(SinkMsg::Shutdown {
            end_ns: self.base.elapsed().as_nanos() as u64,
        });
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

fn run_sink(
    pile_path: PathBuf,
    notebook_title: String,
    session: Id,
    flush_interval: Duration,
    rx: mpsc::Receiver<SinkMsg>,
) -> Result<(), String> {
    if let Some(parent) = pile_path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)
            .map_err(|e| format!("create telemetry pile dir {}: {e}", parent.display()))?;
    }

    let mut pile = Pile::<Blake3>::open(&pile_path)
        .map_err(|e| format!("open telemetry pile {}: {e:?}", pile_path.display()))?;
    pile.restore()
        .map_err(|e| format!("restore telemetry pile {}: {e:?}", pile_path.display()))?;

    let signing_key = SigningKey::generate(&mut OsRng);
    let mut repo = Repository::new(pile, signing_key);

    // Set the default metadata once; commits only carry a handle.
    let metadata_set = schema::build_telemetry_metadata(repo.storage_mut())
        .map_err(|e| format!("build telemetry metadata: {e:?}"))?;
    repo.set_default_metadata(metadata_set)
        .map_err(|e| format!("set default metadata: {e:?}"))?;

    let session_hex = format!("{session:x}");
    let branch_name = format!("telemetry-{}", &session_hex[..8]);
    let branch_id = repo
        .create_branch(&branch_name, None)
        .map_err(|e| format!("create branch {branch_name}: {e:?}"))?
        .release();

    let mut ws = repo
        .pull(branch_id)
        .map_err(|e| format!("pull workspace: {e:?}"))?;

    let session_entity = ExclusiveId::force_ref(&session);
    let mut init = TribleSet::new();
    init += entity! { session_entity @
        schema::kind: schema::kind_session,
        schema::category: "session",
        schema::name: ws.put::<LongString, _>(notebook_title),
        schema::begin_ns: 0u64,
    };
    ws.commit(init, None, Some("telemetry session"));
    push_workspace(&mut repo, &mut ws)?;

    let mut pending = TribleSet::new();
    loop {
        match rx.recv_timeout(flush_interval) {
            Ok(SinkMsg::Begin(msg)) => {
                pending.union(span_begin(&mut ws, msg));
            }
            Ok(SinkMsg::End(msg)) => {
                pending.union(span_end(msg));
            }
            Ok(SinkMsg::Shutdown { end_ns }) => {
                flush(&mut repo, &mut ws, &mut pending)?;
                let session_entity = ExclusiveId::force_ref(&session);
                let mut end = TribleSet::new();
                end += entity! { session_entity @
                    schema::end_ns: end_ns,
                    schema::duration_ns: end_ns,
                };
                ws.commit(end, None, Some("telemetry session end"));
                push_workspace(&mut repo, &mut ws)?;
                break;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                flush(&mut repo, &mut ws, &mut pending)?;
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                flush(&mut repo, &mut ws, &mut pending)?;
                break;
            }
        }
    }

    repo.close()
        .map_err(|e| format!("close telemetry pile {}: {e:?}", pile_path.display()))?;
    Ok(())
}

fn span_begin(
    ws: &mut triblespace::core::repo::Workspace<Pile<Blake3>>,
    msg: BeginMsg,
) -> TribleSet {
    let span_entity = ExclusiveId::force_ref(&msg.span);
    let mut out = TribleSet::new();
    out += entity! { span_entity @
        schema::kind: schema::kind_span,
        schema::session: msg.session,
        schema::category: msg.category,
        schema::name: ws.put::<LongString, _>(msg.name),
        schema::begin_ns: msg.at_ns,
    };
    if let Some(parent) = msg.parent {
        out += entity! { span_entity @ schema::parent: parent };
    }
    if let Some(index) = msg.card_index {
        out += entity! { span_entity @ schema::card_index: index };
    }
    if let Some(source) = msg.source {
        out += entity! { span_entity @ schema::source: ws.put::<LongString, _>(source) };
    }
    out
}

fn span_end(msg: EndMsg) -> TribleSet {
    let span_entity = ExclusiveId::force_ref(&msg.span);
    entity! { span_entity @
        schema::end_ns: msg.at_ns,
        schema::duration_ns: msg.duration_ns,
    }
}

fn flush(
    repo: &mut Repository<Pile<Blake3>>,
    ws: &mut triblespace::core::repo::Workspace<Pile<Blake3>>,
    pending: &mut TribleSet,
) -> Result<(), String> {
    if pending.is_empty() {
        return Ok(());
    }
    let content = std::mem::take(pending);
    ws.commit(content, None, Some("telemetry"));
    push_workspace(repo, ws)?;
    Ok(())
}

fn push_workspace(
    repo: &mut Repository<Pile<Blake3>>,
    ws: &mut triblespace::core::repo::Workspace<Pile<Blake3>>,
) -> Result<(), String> {
    while let Some(mut conflict) = repo
        .try_push(ws)
        .map_err(|e| format!("push telemetry: {e:?}"))?
    {
        conflict
            .merge(ws)
            .map_err(|e| format!("merge push conflict: {e:?}"))?;
        *ws = conflict;
    }
    Ok(())
}
