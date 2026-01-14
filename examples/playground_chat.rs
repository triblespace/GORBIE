#!/usr/bin/env -S watchexec -r rust-script
//! ```cargo
//! [dependencies]
//! GORBIE = { path = ".." }
//! egui = "0.32"
//! triblespace = { path = "../../triblespace-rs" }
//! ed25519-dalek = "2.1.1"
//! rand = "0.8.5"
//! hifitime = "4.2.3"
//! ```

use std::collections::HashMap;
use std::fmt::Debug;
use std::path::PathBuf;

use ed25519_dalek::SigningKey;
use hifitime::Epoch;
use rand::rngs::OsRng;
use triblespace::core::id::ExclusiveId;
use triblespace::core::metadata;
use triblespace::core::repo::pile::Pile;
use triblespace::core::repo::{Repository, Workspace};
use triblespace::macros::id_hex;
use triblespace::prelude::blobschemas::LongString;
use triblespace::prelude::valueschemas::{Blake3, GenId, Handle, NsTAIInterval, ShortString};
use triblespace::prelude::*;

use GORBIE::cards::with_padding;
use GORBIE::dataflow::ComputedState;
use GORBIE::{md, notebook, widgets, Notebook};

mod playground {
    use super::*;

    attributes! {
        "5F10520477A04E5FB322C85CC78C6762" as pub kind: GenId;
        "5A14A02113CE43A59881D0717726F465" as pub request: GenId;
        "B5A6F7B32A254C0A9C6A8B35F60CFC19" as pub conversation: GenId;
        "760AC87BC0F74CDD864DE25C48DAF0E3" as pub status: GenId;
        "DDB3A82DF6C44A3C8D46D5703415DCE4" as pub prompt: Handle<Blake3, LongString>;
        "0DCA39AB16DF4165AB03D1B9EC33A874" as pub response: Handle<Blake3, LongString>;
        "E41A91D2C68640AA86AB31A2CAB2858F" as pub response_raw: Handle<Blake3, LongString>;
        "C1FFE9D4FEC549C09C96639665561DFE" as pub model: ShortString;
        "0DA5DD275AA34F86B0297CC35F1B7395" as pub created_at: NsTAIInterval;
        "8F99CCE853504882A0AD6FFFD9C25B9B" as pub lease_until: NsTAIInterval;
        "0789CCD07EF24F04B74EF72C22BDBB19" as pub worker: ShortString;
        "9E9B829C473E416E9150D4B94A6A2DC4" as pub error: Handle<Blake3, LongString>;
    }

    /// Root id for describing the playground protocol.
    #[allow(non_upper_case_globals)]
    #[allow(dead_code)]
    pub const playground_metadata: Id = id_hex!("C80AA6ED37F85645B2B714A9C184A0F8");

    /// Tag for queued prompt requests.
    #[allow(non_upper_case_globals)]
    pub const kind_request: Id = id_hex!("7D5CF9566906DF9332939F18B789FCBA");
    /// Tag for status updates.
    #[allow(non_upper_case_globals)]
    pub const kind_status: Id = id_hex!("2BFA2349EDAD1860727163D7FB4D2B1C");
    /// Tag for response payloads.
    #[allow(non_upper_case_globals)]
    pub const kind_response: Id = id_hex!("115E05D8453707C90CD5CCF61AD6BCDB");

    /// Request has been queued.
    #[allow(non_upper_case_globals)]
    pub const status_queued: Id = id_hex!("ECDDC1F1CDE8D9CE6008D58C173CA0B5");
    /// Request is being processed by a worker.
    #[allow(non_upper_case_globals)]
    pub const status_in_progress: Id = id_hex!("30374D725D57BB06929A3E3528904248");
    /// Request completed successfully.
    #[allow(non_upper_case_globals)]
    pub const status_done: Id = id_hex!("DFEB4F5697ED89F9C0C19AE0117BF9BE");
    /// Request failed with an error.
    #[allow(non_upper_case_globals)]
    pub const status_failed: Id = id_hex!("C31575855CE25D55EF9DCEE2316ED6E9");
    /// Tag for playground protocol metadata.
    #[allow(non_upper_case_globals)]
    pub const tag_protocol: Id = id_hex!("80CEC2FC7D879BFDA786F862ABB6EFDC");
    /// Tag for kind constants in the playground protocol.
    #[allow(non_upper_case_globals)]
    pub const tag_kind: Id = id_hex!("CA920AA5D7957B737C08EB0F59F24AFA");
    /// Tag for status constants in the playground protocol.
    #[allow(non_upper_case_globals)]
    pub const tag_status: Id = id_hex!("6B9B8AE350D259DD888F49126AA2A1A6");
    /// Tag for tag constants in the playground protocol.
    #[allow(non_upper_case_globals)]
    pub const tag_tag: Id = id_hex!("00E4AD55CD3D8ABA4D4940BC8561DE48");

    #[allow(dead_code)]
    pub fn describe<B>(blobs: &mut B) -> std::result::Result<TribleSet, B::PutError>
    where
        B: BlobStore<Blake3>,
    {
        let mut tribles = TribleSet::new();

        tribles += entity! { ExclusiveId::force_ref(&playground_metadata) @
            metadata::shortname: "playground_metadata",
            metadata::name: blobs.put::<LongString, _>(
                "Root id for describing the playground protocol.".to_string(),
            )?,
            metadata::tag: tag_protocol,
        };

        tribles += entity! { ExclusiveId::force_ref(&tag_protocol) @
            metadata::shortname: "tag_protocol",
            metadata::name: blobs.put::<LongString, _>(
                "Tag for playground protocol metadata.".to_string(),
            )?,
            metadata::tag: tag_tag,
        };

        tribles += entity! { ExclusiveId::force_ref(&tag_kind) @
            metadata::shortname: "tag_kind",
            metadata::name: blobs.put::<LongString, _>(
                "Tag for kind constants in the playground protocol.".to_string(),
            )?,
            metadata::tag: tag_tag,
        };

        tribles += entity! { ExclusiveId::force_ref(&tag_status) @
            metadata::shortname: "tag_status",
            metadata::name: blobs.put::<LongString, _>(
                "Tag for status constants in the playground protocol.".to_string(),
            )?,
            metadata::tag: tag_tag,
        };

        tribles += entity! { ExclusiveId::force_ref(&tag_tag) @
            metadata::shortname: "tag_tag",
            metadata::name: blobs.put::<LongString, _>(
                "Tag for tag constants in the playground protocol.".to_string(),
            )?,
            metadata::tag: tag_tag,
        };

        tribles += entity! { ExclusiveId::force_ref(&kind_request) @
            metadata::shortname: "kind_request",
            metadata::name: blobs.put::<LongString, _>(
                "Tag for queued prompt requests.".to_string(),
            )?,
            metadata::tag: tag_kind,
        };

        tribles += entity! { ExclusiveId::force_ref(&kind_status) @
            metadata::shortname: "kind_status",
            metadata::name: blobs.put::<LongString, _>(
                "Tag for status updates.".to_string(),
            )?,
            metadata::tag: tag_kind,
        };

        tribles += entity! { ExclusiveId::force_ref(&kind_response) @
            metadata::shortname: "kind_response",
            metadata::name: blobs.put::<LongString, _>(
                "Tag for response payloads.".to_string(),
            )?,
            metadata::tag: tag_kind,
        };

        tribles += entity! { ExclusiveId::force_ref(&status_queued) @
            metadata::shortname: "status_queued",
            metadata::name: blobs.put::<LongString, _>(
                "Request has been queued.".to_string(),
            )?,
            metadata::tag: tag_status,
        };

        tribles += entity! { ExclusiveId::force_ref(&status_in_progress) @
            metadata::shortname: "status_in_progress",
            metadata::name: blobs.put::<LongString, _>(
                "Request is being processed by a worker.".to_string(),
            )?,
            metadata::tag: tag_status,
        };

        tribles += entity! { ExclusiveId::force_ref(&status_done) @
            metadata::shortname: "status_done",
            metadata::name: blobs.put::<LongString, _>(
                "Request completed successfully.".to_string(),
            )?,
            metadata::tag: tag_status,
        };

        tribles += entity! { ExclusiveId::force_ref(&status_failed) @
            metadata::shortname: "status_failed",
            metadata::name: blobs.put::<LongString, _>(
                "Request failed with an error.".to_string(),
            )?,
            metadata::tag: tag_status,
        };

        Ok(tribles)
    }
}

#[derive(Debug, Clone)]
struct ChatEntry {
    created_at: i128,
    status: String,
    prompt: String,
    response: Option<String>,
}

#[derive(Debug, Clone)]
struct ChatSnapshot {
    entries: Vec<ChatEntry>,
}

#[derive(Debug, Clone)]
struct RequestInfo {
    prompt: Value<Handle<Blake3, LongString>>,
    created_at: i128,
    conversation: Option<Id>,
}

#[derive(Debug, Clone)]
struct StatusEvent {
    request_id: Id,
    status: Id,
    created_at: i128,
}

#[derive(Debug, Clone)]
struct ResponseInfo {
    created_at: i128,
    handle: Value<Handle<Blake3, LongString>>,
}

struct ChatState {
    pile_path: String,
    pile: Option<Pile<Blake3>>,
    pile_open_path: Option<PathBuf>,
    branch: String,
    model: String,
    prompt: String,
    conversation_id: Id,
    snapshot: ComputedState<Option<Result<ChatSnapshot, String>>>,
    notice: Option<String>,
}

impl Debug for ChatState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChatState")
            .field("pile_path", &self.pile_path)
            .field("pile_open", &self.pile.is_some())
            .field("pile_open_path", &self.pile_open_path)
            .field("branch", &self.branch)
            .field("model", &self.model)
            .field("prompt", &self.prompt)
            .field("conversation_id", &self.conversation_id)
            .field("snapshot", &self.snapshot)
            .field("notice", &self.notice)
            .finish()
    }
}

impl Drop for ChatState {
    fn drop(&mut self) {
        if let Some(pile) = self.pile.take() {
            let _ = pile.close();
        }
    }
}

impl Default for ChatState {
    fn default() -> Self {
        Self {
            pile_path: "./playground.pile".to_owned(),
            pile: None,
            pile_open_path: None,
            branch: "main".to_owned(),
            model: "gpt-4o-mini".to_owned(),
            prompt: String::new(),
            conversation_id: *ufoid(),
            snapshot: ComputedState::default(),
            notice: None,
        }
    }
}

#[notebook]
fn main(nb: &mut Notebook) {
    let default_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "./playground.pile".to_owned());
    let padding = GORBIE::cards::DEFAULT_CARD_PADDING;

    let _chat = nb.state({
            let mut initial = ChatState::default();
            initial.pile_path = default_path;
            initial
        },
        move |ui, state| {
            with_padding(ui, padding, |ui| {
                md!(ui, "# Playground chat\n\nQueue requests into a triblespace pile and view responses.");

                ui.spacing_mut().item_spacing = egui::vec2(8.0, 6.0);
                state.snapshot.poll();

                let mut open_clicked = false;
                ui.horizontal(|ui| {
                    ui.label("Pile:");
                    ui.with_layout(
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            open_clicked = ui
                                .add(widgets::Button::new("Open pile"))
                                .clicked();
                            let field_width = ui.available_width();
                            ui.scope(|ui| {
                                ui.spacing_mut().text_edit_width = field_width;
                                ui.add(widgets::TextField::singleline(
                                    &mut state.pile_path,
                                ));
                            });
                        },
                    );
                });

                ui.columns(2, |columns| {
                    columns[0].horizontal(|ui| {
                        ui.label("Branch:");
                        let field_width = ui.available_width();
                        ui.scope(|ui| {
                            ui.spacing_mut().text_edit_width = field_width;
                            ui.add(widgets::TextField::singleline(
                                &mut state.branch,
                            ));
                        });
                    });
                    columns[1].horizontal(|ui| {
                        ui.label("Model:");
                        let field_width = ui.available_width();
                        ui.scope(|ui| {
                            ui.spacing_mut().text_edit_width = field_width;
                            ui.add(widgets::TextField::singleline(&mut state.model));
                        });
                    });
                });

                if open_clicked {
                    let open_path = PathBuf::from(state.pile_path.trim());
                    match open_pile(&open_path) {
                        Ok(pile) => {
                            let (pile, snapshot) = snapshot_chat(
                                pile,
                                state.branch.trim(),
                                Some(state.conversation_id),
                            );
                            if let Some(old_pile) = state.pile.take() {
                                let _ = old_pile.close();
                            }
                            state.pile = Some(pile);
                            state.pile_open_path = Some(open_path);
                            state.snapshot.set(Some(snapshot));
                        }
                        Err(err) => {
                            state.snapshot.set(Some(Err(err)));
                        }
                    }
                }

                if let Some(pile) = state.pile.take() {
                    let (pile, snapshot) = snapshot_chat(
                        pile,
                        state.branch.trim(),
                        Some(state.conversation_id),
                    );
                    state.pile = Some(pile);
                    state.snapshot.set(Some(snapshot));
                    ui.ctx().request_repaint();
                }

                ui.add_space(10.0);

                let mut send_clicked = false;
                ui.scope(|ui| {
                    let spacing = ui.spacing().item_spacing;
                    ui.spacing_mut().item_spacing = egui::vec2(spacing.x, 2.0);
                    ui.label("Prompt:");
                    let prompt_height =
                        ui.text_style_height(&egui::TextStyle::Body) * 5.0;
                    let prompt_width = ui.available_width();
                    ui.add_sized(
                        [prompt_width, prompt_height],
                        widgets::TextField::multiline(&mut state.prompt),
                    );
                    ui.horizontal(|ui| {
                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                send_clicked =
                                    ui.add(widgets::Button::new("Send")).clicked();
                            },
                        );
                    });
                });

                if send_clicked {
                    let prompt = state.prompt.trim().to_owned();
                    if prompt.is_empty() {
                        state.notice = Some("Prompt is empty.".to_owned());
                    } else {
                        let pile_path = state
                            .pile_open_path
                            .clone()
                            .unwrap_or_else(|| PathBuf::from(state.pile_path.trim()));
                        let branch = state.branch.trim().to_owned();
                        let model = state.model.trim().to_owned();
                        match enqueue_prompt(
                            pile_path,
                            branch,
                            model,
                            state.conversation_id,
                            prompt,
                        ) {
                            Ok(()) => {
                                state.prompt.clear();
                            }
                            Err(err) => state.notice = Some(err),
                        }
                    }
                }

                if let Some(notice) = state.notice.take() {
                    let notice_color = ui.visuals().warn_fg_color;
                    ui.label(
                        egui::RichText::new(notice).color(notice_color),
                    );
                }

                ui.add_space(10.0);
                ui.separator();
                ui.add_space(6.0);

                let snapshot = state.snapshot.value().as_ref();
                if let Some(Ok(snapshot)) = snapshot {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false; 2])
                        .show(ui, |ui| {
                            let label_color = ui.visuals().weak_text_color();
                            for entry in &snapshot.entries {
                                ui.group(|ui| {
                                    ui.spacing_mut().item_spacing = egui::vec2(6.0, 4.0);
                                    ui.label(
                                        egui::RichText::new("User")
                                            .small()
                                            .color(label_color),
                                    );
                                    ui.label(&entry.prompt);
                                    ui.add_space(6.0);
                                    let status = entry.status.as_str();
                                    ui.label(
                                        egui::RichText::new(format!("Status: {status}"))
                                            .small()
                                            .color(label_color),
                                    );
                                    match entry.response.as_deref() {
                                        Some(text) => {
                                            ui.label(
                                                egui::RichText::new("Response")
                                                    .small()
                                                    .color(label_color),
                                            );
                                            ui.label(text);
                                        }
                                        None => {
                                            ui.label(
                                                egui::RichText::new("Response: (pending)")
                                                    .small()
                                                    .color(label_color),
                                            );
                                        }
                                    }
                                });
                                ui.add_space(6.0);
                            }
                        });
                } else if let Some(Err(err)) = snapshot {
                    ui.label(err);
                }
            });
        }
    );
}

fn open_pile(path: &PathBuf) -> Result<Pile<Blake3>, String> {
    let pile_display = path.display();
    let mut pile = map_err_debug(Pile::open(path), &format!("open pile {pile_display}"))?;
    map_err_debug(pile.restore(), "restore pile")?;
    Ok(pile)
}

fn snapshot_chat(
    pile: Pile<Blake3>,
    branch: &str,
    conversation_id: Option<Id>,
) -> (Pile<Blake3>, Result<ChatSnapshot, String>) {
    let mut pile = pile;

    let branch_id = match find_branch_id(&mut pile, branch) {
        Ok(Some(branch_id)) => branch_id,
        Ok(None) => return (pile, Err(format!("branch {branch} not found"))),
        Err(err) => return (pile, Err(err)),
    };
    let mut repo = Repository::new(pile, SigningKey::generate(&mut OsRng));
    let result = (|| -> Result<ChatSnapshot, String> {
        let mut ws = map_err_debug(repo.pull(branch_id), "pull workspace")?;
        let catalog = map_err_debug(ws.checkout(..), "checkout workspace")?;

        let requests = collect_requests(&catalog);
        let statuses = collect_status_events(&catalog);
        let latest = latest_status_by_request(statuses);
        let responses = collect_responses(&catalog);

        let mut entries = Vec::new();
        for (request_id, info) in requests {
            if let (Some(conversation_id), Some(request_conversation)) =
                (conversation_id, info.conversation)
            {
                if request_conversation != conversation_id {
                    continue;
                }
            }
            let prompt = load_text(&mut ws, info.prompt)?;
            let status = latest
                .get(&request_id)
                .map(|event| status_label(event.status))
                .unwrap_or(status_label(playground::status_queued))
                .to_owned();
            let response = responses
                .get(&request_id)
                .map(|response| load_text(&mut ws, response.handle))
                .transpose()?;

            entries.push(ChatEntry {
                created_at: info.created_at,
                status,
                prompt,
                response,
            });
        }

        entries.sort_by_key(|entry| entry.created_at);

        Ok(ChatSnapshot { entries })
    })();
    let pile = repo.into_storage();
    (pile, result)
}

fn enqueue_prompt(
    pile_path: PathBuf,
    branch: String,
    model: String,
    conversation_id: Id,
    prompt: String,
) -> Result<(), String> {
    let mut pile = open_pile(&pile_path)?;
    let branch_id = match find_branch_id(&mut pile, &branch) {
        Ok(branch_id) => branch_id,
        Err(err) => {
            let _ = pile.close();
            return Err(err);
        }
    };
    let mut repo = Repository::new(pile, SigningKey::generate(&mut OsRng));
    let branch_id = match branch_id {
        Some(branch_id) => branch_id,
        None => *map_err_debug(repo.create_branch(&branch, None), "create branch")?,
    };

    let mut ws = map_err_debug(repo.pull(branch_id), "pull workspace")?;
    let now_epoch = now_epoch();
    let now = epoch_interval(now_epoch);
    let request_id = ufoid();
    let prompt_handle = ws.put::<LongString, _>(prompt);

    let mut change = TribleSet::new();
    change += entity! { &request_id @
        playground::kind: playground::kind_request,
        playground::prompt: prompt_handle,
        playground::conversation: conversation_id,
        playground::created_at: now,
        playground::model: model.as_str(),
    };

    change += build_status_event(
        *request_id,
        playground::status_queued,
        now,
        None,
        None,
        None,
    );

    ws.commit(change, Some("enqueue request"));
    let result = map_err_debug(repo.push(&mut ws), "push workspace");
    let pile = repo.into_storage();
    let _ = pile.close();
    result
}

fn find_branch_id(pile: &mut Pile<Blake3>, name: &str) -> Result<Option<Id>, String> {
    let reader = map_err_debug(pile.reader(), "pile reader")?;
    let iter = map_err_debug(pile.branches(), "list branches")?;

    for branch in iter {
        let branch_id = map_err_debug(branch, "branch id")?;
        let Some(head) = map_err_debug(pile.head(branch_id), "branch head")? else {
            continue;
        };
        let metadata_set: TribleSet = map_err_debug(reader.get(head), "branch metadata")?;
        let mut names = find!(
            (shortname: String),
            pattern!(&metadata_set, [{ metadata::shortname: ?shortname }])
        )
        .into_iter();
        let Some((branch_name,)) = names.next() else {
            continue;
        };
        if names.next().is_some() {
            continue;
        }
        if branch_name == name {
            return Ok(Some(branch_id));
        }
    }

    Ok(None)
}

fn collect_requests(catalog: &TribleSet) -> HashMap<Id, RequestInfo> {
    let conversations = find!(
        (request: Id, conversation: Id),
        pattern!(catalog, [{ ?request @ playground::conversation: ?conversation }])
    )
    .into_iter()
    .collect::<HashMap<_, _>>();
    let mut requests = HashMap::new();
    for (request_id, prompt, created_at) in find!(
        (
            request: Id,
            prompt: Value<Handle<Blake3, LongString>>,
            created_at: Value<NsTAIInterval>
        ),
        pattern!(catalog, [{
            ?request @
            playground::kind: playground::kind_request,
            playground::prompt: ?prompt,
            playground::created_at: ?created_at,
        }])
    ) {
        let created_at = interval_key(created_at);
        let conversation = conversations.get(&request_id).copied();
        requests.insert(
            request_id,
            RequestInfo {
                prompt,
                created_at,
                conversation,
            },
        );
    }

    requests
}

fn collect_status_events(catalog: &TribleSet) -> Vec<StatusEvent> {
    let mut events = Vec::new();
    for (_status_id, request_id, status, created_at) in find!(
        (status_id: Id, request: Id, status: Id, created_at: Value<NsTAIInterval>),
        pattern!(catalog, [{
            ?status_id @
            playground::kind: playground::kind_status,
            playground::request: ?request,
            playground::status: ?status,
            playground::created_at: ?created_at,
        }])
    ) {
        let created_at = interval_key(created_at);
        events.push(StatusEvent {
            request_id,
            status,
            created_at,
        });
    }

    events
}

fn latest_status_by_request(events: Vec<StatusEvent>) -> HashMap<Id, StatusEvent> {
    let mut latest = HashMap::new();
    for event in events {
        match latest.get(&event.request_id) {
            None => {
                latest.insert(event.request_id, event);
            }
            Some(existing) if event.created_at > existing.created_at => {
                latest.insert(event.request_id, event);
            }
            _ => {}
        }
    }

    latest
}

fn status_label(status: Id) -> &'static str {
    if status == playground::status_queued {
        "queued"
    } else if status == playground::status_in_progress {
        "in_progress"
    } else if status == playground::status_done {
        "done"
    } else if status == playground::status_failed {
        "failed"
    } else {
        "unknown"
    }
}

fn collect_responses(catalog: &TribleSet) -> HashMap<Id, ResponseInfo> {
    let mut responses: HashMap<Id, ResponseInfo> = HashMap::new();
    for (_response_id, request_id, response, created_at) in find!(
        (
            response_id: Id,
            request: Id,
            response: Value<Handle<Blake3, LongString>>,
            created_at: Value<NsTAIInterval>
        ),
        pattern!(catalog, [{
            ?response_id @
            playground::kind: playground::kind_response,
            playground::request: ?request,
            playground::response: ?response,
            playground::created_at: ?created_at,
        }])
    ) {
        let created_at = interval_key(created_at);
        responses
            .entry(request_id)
            .and_modify(|current| {
                if created_at > current.created_at {
                    *current = ResponseInfo {
                        created_at,
                        handle: response,
                    };
                }
            })
            .or_insert(ResponseInfo {
                created_at,
                handle: response,
            });
    }

    responses
}

fn load_text(
    ws: &mut Workspace<Pile<Blake3>>,
    handle: Value<Handle<Blake3, LongString>>,
) -> Result<String, String> {
    let view = map_err_debug(ws.get::<View<str>, LongString>(handle), "read blob")?;
    Ok(view.to_string())
}

fn build_status_event(
    request_id: Id,
    status: Id,
    now: Value<NsTAIInterval>,
    lease_until: Option<Value<NsTAIInterval>>,
    worker: Option<&str>,
    error: Option<Value<Handle<Blake3, LongString>>>,
) -> TribleSet {
    let status_id = ufoid();
    let mut change = TribleSet::new();
    change += entity! { &status_id @
        playground::kind: playground::kind_status,
        playground::request: request_id,
        playground::status: status,
        playground::created_at: now,
    };

    if let Some(lease_until) = lease_until {
        change += entity! { &status_id @ playground::lease_until: lease_until };
    }

    if let Some(worker) = worker {
        change += entity! { &status_id @ playground::worker: worker };
    }

    if let Some(error) = error {
        change += entity! { &status_id @ playground::error: error };
    }

    change
}

fn now_epoch() -> Epoch {
    Epoch::now().unwrap_or_else(|_| Epoch::from_unix_seconds(0.0))
}

fn epoch_interval(epoch: Epoch) -> Value<NsTAIInterval> {
    (epoch, epoch).to_value()
}

fn epoch_key(epoch: Epoch) -> i128 {
    epoch.to_tai_duration().total_nanoseconds()
}

fn interval_key(interval: Value<NsTAIInterval>) -> i128 {
    let (lower, _): (Epoch, Epoch) = interval.from_value();
    epoch_key(lower)
}

fn map_err_debug<T, E: Debug>(result: Result<T, E>, context: &str) -> Result<T, String> {
    result.map_err(|err| format!("{context}: {err:?}"))
}
