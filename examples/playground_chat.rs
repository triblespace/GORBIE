#!/usr/bin/env -S watchexec -r rust-script
//! ```cargo
//! [dependencies]
//! GORBIE = { path = ".." }
//! egui = "0.32"
//! triblespace = { path = "../../triblespace-rs" }
//! ed25519-dalek = "2.1.1"
//! rand = "0.8.5"
//! num-rational = "0.4.2"
//! ```

use std::collections::HashMap;
use std::fmt::Debug;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ed25519_dalek::SigningKey;
use num_rational::Ratio;
use rand::rngs::OsRng;
use triblespace::core::metadata;
use triblespace::core::repo::pile::Pile;
use triblespace::core::repo::{Repository, Workspace};
use triblespace::prelude::blobschemas::LongString;
use triblespace::prelude::valueschemas::{Blake3, GenId, Handle, R256, ShortString};
use triblespace::prelude::*;

use GORBIE::dataflow::ComputedState;
use GORBIE::{md, notebook, state, widgets};

mod playground {
    use super::*;

    attributes! {
        "5F10520477A04E5FB322C85CC78C6762" as pub kind: ShortString;
        "5A14A02113CE43A59881D0717726F465" as pub request: GenId;
        "760AC87BC0F74CDD864DE25C48DAF0E3" as pub status: ShortString;
        "DDB3A82DF6C44A3C8D46D5703415DCE4" as pub prompt: Handle<Blake3, LongString>;
        "0DCA39AB16DF4165AB03D1B9EC33A874" as pub response: Handle<Blake3, LongString>;
        "E41A91D2C68640AA86AB31A2CAB2858F" as pub response_raw: Handle<Blake3, LongString>;
        "C1FFE9D4FEC549C09C96639665561DFE" as pub model: ShortString;
        "0DA5DD275AA34F86B0297CC35F1B7395" as pub created_at: R256;
        "8F99CCE853504882A0AD6FFFD9C25B9B" as pub lease_until: R256;
        "0789CCD07EF24F04B74EF72C22BDBB19" as pub worker: ShortString;
        "9E9B829C473E416E9150D4B94A6A2DC4" as pub error: Handle<Blake3, LongString>;
    }
}

const KIND_REQUEST: &str = "request";
const KIND_STATUS: &str = "status";
const KIND_RESPONSE: &str = "response";

const STATUS_QUEUED: &str = "queued";

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
}

#[derive(Debug, Clone)]
struct StatusEvent {
    request_id: Id,
    status: String,
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
    snapshot: ComputedState<Result<ChatSnapshot, String>>,
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
            snapshot: ComputedState::Undefined,
            notice: None,
        }
    }
}

#[notebook]
fn main() {
    let default_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "./playground.pile".to_owned());
    let padding = GORBIE::cards::DEFAULT_CARD_PADDING;

    state!(
        _chat = {
            let mut initial = ChatState::default();
            initial.pile_path = default_path;
            initial
        },
        move |ui, state| {
            ui.with_padding(padding, |ui| {
                md!(ui, "# Playground chat\n\nQueue requests into a triblespace pile and view responses.");

                ui.spacing_mut().item_spacing = egui::vec2(8.0, 6.0);

                let mut open_clicked = false;
                ui.horizontal(|ui| {
                    ui.label("Pile:");
                    ui.with_layout(
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            open_clicked =
                                ui.add(widgets::Button::new("Open pile")).clicked();
                            let field_width = ui.available_width();
                            ui.scope(|ui| {
                                ui.spacing_mut().text_edit_width = field_width;
                                ui.add(widgets::TextField::singleline(&mut state.pile_path));
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
                            ui.add(widgets::TextField::singleline(&mut state.branch));
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
                            let (pile, snapshot) = snapshot_chat(pile, state.branch.trim());
                            if let Some(old_pile) = state.pile.take() {
                                let _ = old_pile.close();
                            }
                            state.pile = Some(pile);
                            state.pile_open_path = Some(open_path);
                            state.snapshot = ComputedState::Ready(snapshot, 0);
                        }
                        Err(err) => {
                            state.snapshot = ComputedState::Ready(Err(err), 0);
                        }
                    }
                }

                if let Some(pile) = state.pile.take() {
                    let (pile, snapshot) = snapshot_chat(pile, state.branch.trim());
                    state.pile = Some(pile);
                    state.snapshot = ComputedState::Ready(snapshot, 0);
                    ui.ctx().request_repaint();
                }

                ui.add_space(10.0);

                let mut send_clicked = false;
                ui.label("Prompt:");
                let prompt_height = ui.text_style_height(&egui::TextStyle::Body) * 5.0;
                let prompt_width = ui.available_width();
                ui.add_sized(
                    [prompt_width, prompt_height],
                    widgets::TextField::multiline(&mut state.prompt),
                );
                ui.add_space(2.0);
                ui.horizontal(|ui| {
                    ui.with_layout(
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            send_clicked =
                                ui.add(widgets::Button::new("Send")).clicked();
                        },
                    );
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
                        match enqueue_prompt(pile_path, branch, model, prompt) {
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

                let snapshot = state.snapshot.ready();
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

fn snapshot_chat(pile: Pile<Blake3>, branch: &str) -> (Pile<Blake3>, Result<ChatSnapshot, String>) {
    let mut pile = pile;
    if let Err(err) = map_err_debug(pile.refresh(), "refresh pile") {
        return (pile, Err(err));
    }

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
            let prompt = load_text(&mut ws, info.prompt)?;
            let status = latest
                .get(&request_id)
                .map(|event| event.status.as_str())
                .unwrap_or(STATUS_QUEUED)
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
    let now = now_ms();
    let request_id = ufoid();
    let prompt_handle = ws.put::<LongString, _>(prompt);

    let mut change = TribleSet::new();
    change += entity! { &request_id @
        playground::kind: KIND_REQUEST,
        playground::prompt: prompt_handle,
        playground::created_at: now,
        playground::model: model.as_str(),
    };

    change += build_status_event(*request_id, STATUS_QUEUED, now, None, None, None);

    ws.commit(change, Some("enqueue request"));
    let result = push_with_merge(&mut repo, ws);
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
    let mut requests = HashMap::new();
    for (request_id, prompt, created_at) in find!(
        (request: Id, prompt: Value<Handle<Blake3, LongString>>, created_at: Ratio<i128>),
        pattern!(catalog, [{
            ?request @
            playground::kind: KIND_REQUEST,
            playground::prompt: ?prompt,
            playground::created_at: ?created_at,
        }])
    ) {
        let created_at = ratio_to_i128(created_at).unwrap_or_default();
        requests.insert(
            request_id,
            RequestInfo {
                prompt,
                created_at,
            },
        );
    }

    requests
}

fn collect_status_events(catalog: &TribleSet) -> Vec<StatusEvent> {
    let mut events = Vec::new();
    for (_status_id, request_id, status, created_at) in find!(
        (status_id: Id, request: Id, status: String, created_at: Ratio<i128>),
        pattern!(catalog, [{
            ?status_id @
            playground::kind: KIND_STATUS,
            playground::request: ?request,
            playground::status: ?status,
            playground::created_at: ?created_at,
        }])
    ) {
        let created_at = ratio_to_i128(created_at).unwrap_or_default();
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

fn collect_responses(catalog: &TribleSet) -> HashMap<Id, ResponseInfo> {
    let mut responses: HashMap<Id, ResponseInfo> = HashMap::new();
    for (_response_id, request_id, response, created_at) in find!(
        (
            response_id: Id,
            request: Id,
            response: Value<Handle<Blake3, LongString>>,
            created_at: Ratio<i128>
        ),
        pattern!(catalog, [{
            ?response_id @
            playground::kind: KIND_RESPONSE,
            playground::request: ?request,
            playground::response: ?response,
            playground::created_at: ?created_at,
        }])
    ) {
        let created_at = ratio_to_i128(created_at).unwrap_or_default();
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
    status: &str,
    now: i128,
    lease_until: Option<i128>,
    worker: Option<&str>,
    error: Option<Value<Handle<Blake3, LongString>>>,
) -> TribleSet {
    let status_id = ufoid();
    let mut change = TribleSet::new();
    change += entity! { &status_id @
        playground::kind: KIND_STATUS,
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

fn push_with_merge(repo: &mut Repository<Pile<Blake3>>, mut ws: Workspace<Pile<Blake3>>) -> Result<(), String> {
    loop {
        match map_err_debug(repo.try_push(&mut ws), "push workspace")? {
            None => return Ok(()),
            Some(mut other) => {
                map_err_debug(other.merge(&mut ws), "merge workspace")?;
                ws = other;
            }
        }
    }
}

fn now_ms() -> i128 {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0));
    duration.as_millis() as i128
}

fn ratio_to_i128(ratio: Ratio<i128>) -> Option<i128> {
    if *ratio.denom() == 1 {
        Some(*ratio.numer())
    } else {
        None
    }
}

fn map_err_debug<T, E: Debug>(result: Result<T, E>, context: &str) -> Result<T, String> {
    result.map_err(|err| format!("{context}: {err:?}"))
}
