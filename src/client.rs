use std::{
    cell::Cell,
    io::{self, Read, Write},
    time::{Duration, Instant},
};

use crossterm::{
    cursor::Show,
    event::{
        DisableMouseCapture, EnableMouseCapture, Event, EventStream, KeyCode, KeyEvent,
        KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
    },
    execute, terminal,
};
use futures_util::StreamExt;
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Terminal,
};
use tokio::{
    sync::{mpsc, Notify},
    time,
};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::{
    error::Result,
    network,
    protocol::{
        HistoryKind, HistorySection, InboxItem, InboxPage, Message, MessageReply, NoticeItem,
        NoticePage, Operation, PendingRequest, RequestPage, Target,
    },
};

const READ_TIMEOUT_MS: i32 = 1_000;

fn rejection(error: &crate::error::AppError) -> bool {
    matches!(error, crate::error::AppError::Remote(message)
        if matches!(message.as_str(), "invalid request" | "identity conflict" | "request not found" | "request already answered" | "invalid option"))
}

async fn submit_checked(address: &str, operation: &Operation) -> Result<serde_json::Value> {
    let response = network::call(address, operation).await?;
    let id = response.get("id").and_then(serde_json::Value::as_i64);
    let valid = match operation {
        Operation::SendMessage { .. } => id.is_some_and(|id| id > 0),
        Operation::ReplyMessage { reply } => {
            id.is_some_and(|id| id > 0)
                && response
                    .get("request_id")
                    .and_then(serde_json::Value::as_i64)
                    == Some(reply.request_id)
        }
        Operation::Answer { id: expected, .. } | Operation::AnswerCustom { id: expected, .. } => {
            id == Some(*expected)
        }
        Operation::DeferDecision { id: expected, .. } => {
            id == Some(*expected)
                && response
                    .get("event_id")
                    .and_then(serde_json::Value::as_i64)
                    .is_some_and(|id| id > 0)
        }
        Operation::SetImportance {
            id: expected,
            importance,
        } => {
            id == Some(*expected)
                && response
                    .get("importance")
                    .and_then(serde_json::Value::as_u64)
                    == Some(u64::from(*importance))
        }
        Operation::DismissNotice { id: expected } => {
            id == Some(*expected)
                && response
                    .get("dismissed")
                    .and_then(serde_json::Value::as_bool)
                    == Some(true)
        }
        _ => false,
    };
    if valid {
        Ok(response)
    } else {
        Err(crate::error::AppError::InvalidFrame)
    }
}

pub async fn watch(address: &str) -> Result<()> {
    let (_watch, mut changes) = network::watch_changes(address);
    while changes.changed().await.is_ok() {
        let state = *changes.borrow_and_update();
        println!(
            "{}",
            serde_json::json!({"connected": state.connected, "revision": state.revision})
        );
        io::stdout().flush()?;
    }
    Err(crate::error::AppError::Task(
        "change subscription closed".into(),
    ))
}

pub async fn wait_for_answer(address: &str, id: i64, after: i64) -> Result<()> {
    let (_watch, mut changes) = network::watch_changes(address);
    loop {
        let response = network::call(address, &Operation::DecisionStatus { id, after }).await?;
        if response.get("id").and_then(serde_json::Value::as_i64) != Some(id) {
            return Err(crate::error::AppError::InvalidFrame);
        }
        let answer = response
            .get("answer")
            .ok_or(crate::error::AppError::InvalidFrame)?;
        let event = response
            .get("deferred_event")
            .ok_or(crate::error::AppError::InvalidFrame)?;
        if !answer.is_null() {
            println!("{}", serde_json::json!({"id": id, "answer": answer}));
            return Ok(());
        }
        if let Some(event_id) = event.as_i64().filter(|event_id| *event_id > 0) {
            println!(
                "{}",
                serde_json::json!({"id": id, "answer": null, "deferred_event": event_id})
            );
            return Ok(());
        }
        if !event.is_null() {
            return Err(crate::error::AppError::InvalidFrame);
        }
        changes
            .changed()
            .await
            .map_err(|_| crate::error::AppError::Task("change subscription closed".into()))?;
        if !changes.borrow_and_update().connected {
            return Err(crate::error::AppError::Task(
                "live updates disconnected".into(),
            ));
        }
    }
}

pub async fn wait_for_message_response(address: &str, id: i64) -> Result<()> {
    let (_watch, mut changes) = network::watch_changes(address);
    loop {
        let response = network::call(address, &Operation::GetMessageResponse { id }).await?;
        if response.get("id").and_then(serde_json::Value::as_i64) != Some(id) {
            return Err(crate::error::AppError::InvalidFrame);
        }
        let value = response
            .get("response")
            .ok_or(crate::error::AppError::InvalidFrame)?;
        if !value.is_null() {
            println!("{response}");
            return Ok(());
        }
        changes
            .changed()
            .await
            .map_err(|_| crate::error::AppError::Task("change subscription closed".into()))?;
        if !changes.borrow_and_update().connected {
            return Err(crate::error::AppError::Task(
                "live updates disconnected".into(),
            ));
        }
    }
}

async fn fetch_requests(address: &str, offset: usize) -> Result<RequestPage> {
    let page: RequestPage = serde_json::from_value(
        network::call_with_timeout(
            address,
            &Operation::DecisionQueue { offset },
            READ_TIMEOUT_MS,
        )
        .await?,
    )?;
    for request in &page.requests {
        request.request.validate()?;
    }
    if page.next_offset.is_some_and(|next| next <= offset) {
        return Err(crate::error::AppError::InvalidFrame);
    }
    Ok(page)
}

async fn fetch_notices(address: &str, offset: usize) -> Result<NoticePage> {
    let page: NoticePage = serde_json::from_value(
        network::call_with_timeout(
            address,
            &Operation::NoticesPage {
                include_dismissed: false,
                offset,
            },
            READ_TIMEOUT_MS,
        )
        .await?,
    )?;
    if page.next_offset.is_some_and(|next| next <= offset) {
        return Err(crate::error::AppError::InvalidFrame);
    }
    Ok(page)
}

async fn fetch_operator_inbox(address: &str, offset: usize) -> Result<InboxPage> {
    let page: InboxPage = serde_json::from_value(
        network::call_with_timeout(
            address,
            &Operation::InboxPage {
                participant: "operator".into(),
                offset,
            },
            READ_TIMEOUT_MS,
        )
        .await?,
    )?;
    if page.next_offset.is_some_and(|next| next <= offset) {
        return Err(crate::error::AppError::InvalidFrame);
    }
    Ok(page)
}

struct HistoryResult {
    items: Vec<serde_json::Value>,
    next_offset: Option<usize>,
}

fn history_section(index: usize) -> (HistorySection, &'static str) {
    match index {
        0 => (HistorySection::Decisions, "decisions"),
        1 => (HistorySection::Messages, "messages"),
        _ => (HistorySection::Outcomes, "outcomes"),
    }
}

async fn fetch_history(
    address: &str,
    kind: HistoryKind,
    id: i64,
    section: usize,
    offset: usize,
    root_first: bool,
) -> Result<HistoryResult> {
    let (section, field) = history_section(section);
    let mut response = network::call_with_timeout(
        address,
        &Operation::HistoryPage {
            kind,
            id,
            section,
            offset,
            root_first,
        },
        READ_TIMEOUT_MS,
    )
    .await?;
    if response.get("id").and_then(serde_json::Value::as_i64) != Some(id)
        || response.get("section").and_then(serde_json::Value::as_str) != Some(field)
    {
        return Err(crate::error::AppError::InvalidFrame);
    }
    let object = response
        .as_object_mut()
        .ok_or(crate::error::AppError::InvalidFrame)?;
    let items = serde_json::from_value(
        object
            .remove(field)
            .ok_or(crate::error::AppError::InvalidFrame)?,
    )?;
    let next_offset: Option<usize> = serde_json::from_value(
        object
            .remove("next_offset")
            .ok_or(crate::error::AppError::InvalidFrame)?,
    )?;
    if next_offset.is_some_and(|next| next <= offset) {
        return Err(crate::error::AppError::InvalidFrame);
    }
    Ok(HistoryResult { items, next_offset })
}

#[derive(Default)]
struct HistoryLookup {
    kind: Option<HistoryKind>,
    id: String,
    invalid: bool,
}

#[derive(serde::Deserialize)]
struct SearchItem {
    id: i64,
    sender: String,
    body: String,
    context: String,
}

#[derive(serde::Deserialize)]
struct SearchPage {
    messages: Vec<SearchItem>,
    next_after: Option<i64>,
}

#[derive(serde::Deserialize)]
struct OverviewCounts {
    decisions: u64,
    messages: u64,
    notices: u64,
}

#[derive(Default)]
struct OverviewView {
    counts: Option<OverviewCounts>,
    error: Option<String>,
    dirty: bool,
    loading: bool,
}

async fn fetch_overview(address: &str) -> Result<OverviewCounts> {
    let value = network::call_with_timeout(address, &Operation::Overview, READ_TIMEOUT_MS).await?;
    Ok(serde_json::from_value(value)?)
}

#[derive(Default)]
struct SearchView {
    query: String,
    editing: bool,
    loading: bool,
    after: i64,
    previous: Vec<i64>,
    page: Option<SearchPage>,
    selected: usize,
    error: Option<String>,
}

struct SuspendedAnswer {
    request: PendingRequest,
    text: String,
    offset: usize,
    previous: Vec<usize>,
    scroll: usize,
    anchor: (usize, usize, usize),
    follow_answer: bool,
}

async fn fetch_search(address: &str, query: String, after: i64) -> Result<SearchPage> {
    let value = network::call_with_timeout(
        address,
        &Operation::SearchMessages { query, after },
        READ_TIMEOUT_MS,
    )
    .await?;
    let page: SearchPage = serde_json::from_value(value)?;
    if page.messages.len() > 25
        || page.messages.iter().any(|item| item.id <= after)
        || page.messages.windows(2).any(|pair| {
            pair.first()
                .zip(pair.last())
                .is_some_and(|(a, b)| a.id >= b.id)
        })
        || page
            .next_after
            .is_some_and(|next| page.messages.last().is_none_or(|item| item.id != next))
    {
        return Err(crate::error::AppError::InvalidFrame);
    }
    Ok(page)
}

struct HistoryView {
    kind: HistoryKind,
    id: i64,
    root_first: bool,
    section: usize,
    offset: usize,
    previous: Vec<usize>,
    items: Vec<serde_json::Value>,
    next_offset: Option<usize>,
    scroll: Cell<usize>,
    anchor: Cell<(usize, usize, usize)>,
    max_scroll: Cell<usize>,
    loading: bool,
    needs_load: bool,
    error: Option<String>,
}
impl HistoryView {
    fn new(kind: HistoryKind, id: i64) -> Self {
        Self {
            kind,
            id,
            root_first: false,
            section: 0,
            offset: 0,
            previous: Vec::new(),
            items: Vec::new(),
            next_offset: None,
            scroll: Cell::new(0),
            anchor: Cell::new((0, 0, 0)),
            max_scroll: Cell::new(0),
            loading: true,
            needs_load: true,
            error: None,
        }
    }
    fn reload(&mut self) {
        self.offset = 0;
        self.items.clear();
        self.next_offset = None;
        self.scroll.set(0);
        self.anchor.set((0, 0, 0));
        self.max_scroll.set(0);
        self.loading = true;
        self.needs_load = true;
        self.error = None;
    }
}

// Crossterm emits indexed-color SGR even for its named basic colors. Convert
// only the two accents used by this interface to their eight-color equivalents.
// Buffer through flush so a sequence split across Write calls is still intact.
struct BasicColorWriter<W> {
    inner: W,
    pending: Vec<u8>,
}
impl<W: Write> Write for BasicColorWriter<W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.pending.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        let mut result = Vec::with_capacity(self.pending.len());
        let mut pos = 0;
        while pos < self.pending.len() {
            let rest = &self.pending[pos..];
            let mut replaced = false;
            for (indexed, basic) in [
                (b"\x1b[38;5;6".as_slice(), b"\x1b[36".as_slice()),
                (b"\x1b[38;5;11".as_slice(), b"\x1b[33".as_slice()),
            ] {
                if rest.starts_with(indexed) && matches!(rest.get(indexed.len()), Some(b'm' | b';'))
                {
                    result.extend_from_slice(basic);
                    pos += indexed.len();
                    replaced = true;
                    break;
                }
            }
            if !replaced {
                result.push(self.pending[pos]);
                pos += 1;
            }
        }
        self.inner.write_all(&result)?;
        self.pending.clear();
        self.inner.flush()
    }
}

type Screen = Terminal<CrosstermBackend<BasicColorWriter<io::Stdout>>>;

// Restore the terminal and stop the input thread on every ordinary exit path.
struct TerminalGuard {
    input: tokio::task::JoinHandle<()>,
}
impl Drop for TerminalGuard {
    fn drop(&mut self) {
        self.input.abort();
        let _ = execute!(
            io::stdout(),
            DisableMouseCapture,
            terminal::LeaveAlternateScreen,
            Show
        );
        let _ = terminal::disable_raw_mode();
        let _ = io::stdout().flush();
    }
}

#[derive(Default)]
struct Composer {
    reply_to: Option<i64>,
    stage: usize,
    review_page: usize,
    fields: [String; 4],
    error: Option<String>,
    cursor: usize,
    confirm_discard: bool,
}
impl Composer {
    fn instruction() -> io::Result<Self> {
        let mut random = [0u8; 16];
        std::fs::File::open("/dev/urandom")?.read_exact(&mut random)?;
        let identity = random
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        Ok(Self {
            fields: [
                format!("tui-{identity}"),
                "all".into(),
                String::new(),
                String::new(),
            ],
            ..Self::default()
        })
    }
    fn review_stage(&self) -> usize {
        if self.reply_to.is_some() {
            2
        } else {
            1
        }
    }
    fn editing_field(&self) -> usize {
        if self.reply_to.is_some() {
            self.stage
        } else {
            3
        }
    }
    fn reply(&self) -> Option<MessageReply> {
        let request_id = self.reply_to?;
        let identity = self.fields.first()?.clone();
        let body = self.fields.get(1)?.clone();
        if identity.is_empty() || body.is_empty() {
            return None;
        }
        Some(MessageReply {
            identity,
            request_id,
            sender: "operator".into(),
            body,
        })
    }
    fn message(&self) -> Option<Message> {
        let [identity, channel, target, body] = &self.fields;
        if identity.is_empty() || channel.is_empty() || body.is_empty() {
            return None;
        }
        Some(Message {
            identity: identity.clone(),
            channel: channel.clone(),
            sender: "operator".into(),
            target: if target.is_empty() {
                Target::Group
            } else {
                Target::Participant {
                    name: target.clone(),
                }
            },
            body: body.clone(),
            expects_reply: false,
            context: None,
            activity: None,
        })
    }
}

#[derive(Default)]
struct Arrivals {
    seen: bool,
    highest_id: i64,
    pending: bool,
}
impl Arrivals {
    fn observe(&mut self, newest: Option<i64>) -> bool {
        let newest = newest.unwrap_or(0);
        let changed = self.seen && newest > self.highest_id && !self.pending;
        if self.seen && newest > self.highest_id {
            self.pending = true;
        }
        self.highest_id = self.highest_id.max(newest);
        self.seen = true;
        changed
    }
}

struct View {
    help: bool,
    help_page: usize,
    page_size: usize,
    requests: Vec<PendingRequest>,
    request_offset: usize,
    request_previous: Vec<usize>,
    request_next: Option<usize>,
    request_loaded: bool,
    request_select_last: Option<bool>,
    arrivals: Arrivals,
    composer: Option<Composer>,
    suspended_instruction: Option<Composer>,
    suspended_answer: Option<SuspendedAnswer>,
    status: Option<String>,
    status_expires: Option<Instant>,
    connection_unavailable: bool,
    poll_error: Option<String>,
    poll_summary: Option<String>,
    busy: bool,
    list_focus: bool,
    detail_scroll: Cell<usize>,
    inbox_scroll: Cell<usize>,
    notice_scroll: Cell<usize>,
    detail_anchor: Cell<(usize, usize, usize)>,
    inbox_anchor: Cell<(usize, usize, usize)>,
    notice_anchor: Cell<(usize, usize, usize)>,
    detail_max: Cell<usize>,
    detail_actual: Cell<usize>,
    follow_answer: bool,
    history: Option<HistoryView>,
    history_lookup: Option<HistoryLookup>,
    search: Option<SearchView>,
    overview: Option<OverviewView>,
    notices: Vec<NoticeItem>,
    notice_view: bool,
    notice_loaded: bool,
    notice_offset: usize,
    notice_previous: Vec<usize>,
    notice_next: Option<usize>,
    notice_select_last: Option<bool>,
    inbox_view: bool,
    inbox_loaded: bool,
    inbox_offset: usize,
    inbox_previous: Vec<usize>,
    inbox_next: Option<usize>,
    inbox_select_last: Option<bool>,
    inbox: Vec<InboxItem>,
    inbox_current: usize,
    inbox_page: usize,
    notice_current: usize,
    notice_page: usize,
    current: usize,
    defer_retry: Option<(i64, String)>,
    priority_edit: bool,
    detail_page: Option<usize>,
    detail_option: Option<usize>,
    selected: Option<usize>,
    editing: bool,
    custom: String,
    draft_request_missing: bool,
}
impl View {
    fn mouse(&mut self, mouse: MouseEvent, width: u16, height: u16) -> Action {
        if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left))
            || self.busy
            || self.composer.is_some()
            || self.suspended_instruction.is_some()
            || self.editing
            || self.priority_edit
            || self.history.is_some()
            || self.history_lookup.is_some()
            || self.search.is_some()
            || self.overview.is_some()
            || self.help
            || self.detail_page.is_some()
            || width < 40
            || height < 12
        {
            return Action::None;
        }
        let x = mouse.column;
        let y = mouse.row;
        // The first row is the heading. These labels are also the mouse
        // navigation targets, including their compact forms on narrow panes.
        if y == 0 {
            if width < 60 {
                let inbox_x = 8u16.saturating_add(self.title_width());
                let notice_x = inbox_x.saturating_add(4);
                if x >= notice_x && x < notice_x.saturating_add(2) {
                    return Action::ToggleNotices;
                }
                if x >= inbox_x && x < inbox_x.saturating_add(3) {
                    return Action::ToggleInbox;
                }
            } else {
                let inbox_x = 11u16.saturating_add(self.title_width());
                let notice_x = inbox_x.saturating_add(8);
                if x >= notice_x && x < notice_x.saturating_add(10) {
                    return Action::ToggleNotices;
                }
                if x >= inbox_x && x < inbox_x.saturating_add(8) {
                    return Action::ToggleInbox;
                }
            }
            return Action::None;
        }
        let has_refresh = self.connection_unavailable
            || if width < 60 {
                self.poll_summary.is_some()
            } else {
                self.poll_error.is_some()
            };
        let has_status = self.busy
            || self.status.is_some()
            || self.suspended_instruction.is_some()
            || self.suspended_answer.is_some();
        let footer_height = 2
            + u16::from(has_refresh && (has_status || self.draft_request_missing))
            + u16::from(self.draft_request_missing && has_status);
        let content_height = height.saturating_sub(1 + footer_height);
        if y == 1 || y > content_height {
            return Action::None;
        }
        let horizontal = width >= 70;
        let list_width = if horizontal { width * 35 / 100 } else { width };
        let list_height = if horizontal { content_height } else { 4 };
        let in_list = if horizontal {
            x < list_width
        } else {
            y < 1 + list_height
        };
        if in_list {
            self.list_focus = true;
            if y >= 2 && y < 1 + list_height.saturating_sub(1) {
                let rows = if self.inbox_view {
                    self.inbox.len()
                } else if self.notice_view {
                    self.notices.len()
                } else {
                    self.requests.len()
                };
                let row_index = usize::from(y.saturating_sub(2));
                let capacity = usize::from(list_height.saturating_sub(3)).max(1);
                let start = self
                    .selected_index()
                    .saturating_sub(capacity.saturating_sub(1));
                let selected = (start + row_index).min(rows.saturating_sub(1));
                if rows > 0 && selected < rows {
                    if self.inbox_view {
                        self.inbox_current = selected;
                    } else if self.notice_view {
                        self.notice_current = selected;
                    } else {
                        self.current = selected;
                        self.reset_selection();
                    }
                }
            }
        } else {
            self.list_focus = false;
        }
        Action::Redraw
    }

    fn title_width(&self) -> u16 {
        let title = if self.inbox_view {
            "Operator inbox"
        } else if self.notice_view {
            "Completion notices"
        } else {
            "Decisions"
        };
        title.len() as u16
    }

    fn selected_index(&self) -> usize {
        if self.inbox_view {
            self.inbox_current
        } else if self.notice_view {
            self.notice_current
        } else {
            self.current
        }
    }

    fn new() -> Self {
        Self {
            help: false,
            help_page: 0,
            page_size: detail_page_size(),
            requests: Vec::new(),
            request_offset: 0,
            request_previous: Vec::new(),
            request_next: None,
            request_loaded: false,
            request_select_last: None,
            arrivals: Arrivals::default(),
            composer: None,
            suspended_instruction: None,
            suspended_answer: None,
            status: None,
            status_expires: None,
            connection_unavailable: false,
            poll_error: None,
            poll_summary: None,
            busy: false,
            list_focus: false,
            detail_scroll: Cell::new(0),
            inbox_scroll: Cell::new(0),
            notice_scroll: Cell::new(0),
            detail_anchor: Cell::new((0, 0, 0)),
            inbox_anchor: Cell::new((0, 0, 0)),
            notice_anchor: Cell::new((0, 0, 0)),
            detail_max: Cell::new(0),
            detail_actual: Cell::new(0),
            follow_answer: true,
            history: None,
            history_lookup: None,
            search: None,
            overview: None,
            notices: Vec::new(),
            notice_view: false,
            notice_loaded: false,
            notice_offset: 0,
            notice_previous: Vec::new(),
            notice_next: None,
            notice_select_last: None,
            inbox_view: false,
            inbox_loaded: false,
            inbox_offset: 0,
            inbox_previous: Vec::new(),
            inbox_next: None,
            inbox_select_last: None,
            inbox: Vec::new(),
            inbox_current: 0,
            inbox_page: 0,
            notice_current: 0,
            notice_page: 0,
            current: 0,
            defer_retry: None,
            priority_edit: false,
            detail_page: None,
            detail_option: None,
            selected: None,
            editing: false,
            custom: String::new(),
            draft_request_missing: false,
        }
    }
    fn current(&self) -> Option<&PendingRequest> {
        self.requests.get(self.current)
    }
    fn resize_pages(&mut self, new_size: usize) {
        let old_size = self.page_size;
        let remap = |page: &mut usize| *page = page.saturating_mul(old_size) / new_size;
        if let Some(page) = &mut self.detail_page {
            remap(page);
        }
        if let Some(composer) = &mut self.composer {
            remap(&mut composer.review_page);
        }
        remap(&mut self.inbox_page);
        remap(&mut self.notice_page);
        self.page_size = new_size;
    }
    fn reset_selection(&mut self) {
        self.detail_scroll.set(0);
        self.detail_anchor.set((0, 0, 0));
        self.follow_answer = true;
        self.selected = self
            .current()
            .and_then(|r| r.request.recommendation.map(|_| 0));
        self.editing = false;
        self.priority_edit = false;
        self.detail_page = None;
        self.detail_option = None;
        self.custom.clear();
        self.draft_request_missing = false;
    }
    fn update(&mut self, requests: Vec<PendingRequest>) -> bool {
        // Keep the request and draft visible if a concurrent change removes it
        // from this queue. Submitting it can still fail; never silently retarget it.
        if self.editing
            && self
                .current()
                .is_some_and(|current| !requests.iter().any(|r| r.id == current.id))
        {
            let changed = !self.draft_request_missing;
            self.draft_request_missing = true;
            return changed;
        }
        let missing_changed = std::mem::take(&mut self.draft_request_missing);
        if self.requests == requests {
            return missing_changed;
        }
        let previous = self.current().map(|r| r.id);
        self.current = previous
            .and_then(|id| requests.iter().position(|r| r.id == id))
            .unwrap_or(0);
        let changed = previous != requests.get(self.current).map(|r| r.id);
        self.requests = requests;
        if changed {
            self.reset_selection();
        }
        true
    }
    fn reset_request_page(&mut self) {
        self.request_offset = 0;
        self.request_previous.clear();
        self.request_next = None;
        self.request_loaded = false;
        self.request_select_last = None;
        self.update(Vec::new());
    }
    fn update_request_page(&mut self, page: RequestPage) -> bool {
        let arrival_changed = self.arrivals.observe(page.newest_id);
        if arrival_changed {
            self.arrivals.pending = false;
            if self.status.is_none() || self.status_expires.is_some() {
                self.status = Some("New decision received".into());
                self.status_expires = Some(Instant::now() + Duration::from_secs(4));
            }
        }
        if !self.editing && page.requests.is_empty() && self.request_offset > 0 {
            self.request_offset = self.request_previous.pop().unwrap_or(0);
            self.request_loaded = false;
            self.request_select_last = Some(true);
            self.update(Vec::new());
            self.request_next = None;
            return true;
        }
        let was_loaded = self.request_loaded;
        let next_changed = self.request_next != page.next_offset;
        let changed = self.update(page.requests);
        if self.draft_request_missing {
            return changed || arrival_changed;
        }
        if let Some(last) = self.request_select_last.take() {
            self.current = if last {
                self.requests.len().saturating_sub(1)
            } else {
                0
            };
            self.reset_selection();
        }
        self.request_next = page.next_offset;
        self.request_loaded = true;
        changed || next_changed || !was_loaded || arrival_changed
    }
    fn update_notices(&mut self, page: NoticePage) -> bool {
        // Dismissals on earlier pages can move the end before this offset.
        if page.notices.is_empty() && self.notice_offset > 0 {
            self.notice_offset = self.notice_previous.pop().unwrap_or(0);
            self.notice_loaded = false;
            self.notice_select_last = Some(true);
            self.notices.clear();
            self.notice_next = None;
            return true;
        }
        let changed = !self.notice_loaded
            || self.notices != page.notices
            || self.notice_next != page.next_offset;
        let previous = self.notices.get(self.notice_current).map(|item| item.id);
        let current = match self.notice_select_last.take() {
            Some(true) => page.notices.len().saturating_sub(1),
            Some(false) => 0,
            None => previous
                .and_then(|id| page.notices.iter().position(|item| item.id == id))
                .unwrap_or(0),
        };
        if previous != page.notices.get(current).map(|item| item.id) {
            self.notice_page = 0;
            self.notice_scroll.set(0);
            self.notice_anchor.set((0, 0, 0));
        }
        self.notice_current = current;
        self.notices = page.notices;
        self.notice_next = page.next_offset;
        self.notice_loaded = true;
        changed
    }
    fn reset_notice_page(&mut self) {
        self.notice_offset = 0;
        self.notice_previous.clear();
        self.notice_loaded = false;
        self.notice_select_last = Some(false);
        self.notice_current = 0;
        self.notice_page = 0;
    }
    fn update_inbox(&mut self, page: InboxPage) -> bool {
        let first_load = !self.inbox_loaded;
        let changed =
            first_load || self.inbox != page.messages || self.inbox_next != page.next_offset;
        let previous = self.inbox.get(self.inbox_current).map(|item| item.id);
        let current = match self.inbox_select_last.take() {
            Some(true) => page.messages.len().saturating_sub(1),
            Some(false) => 0,
            None => previous
                .and_then(|id| page.messages.iter().position(|item| item.id == id))
                .unwrap_or(0),
        };
        if previous != page.messages.get(current).map(|item| item.id) {
            self.inbox_page = 0;
            self.inbox_scroll.set(0);
            self.inbox_anchor.set((0, 0, 0));
        }
        self.inbox_current = current;
        self.inbox = page.messages;
        self.inbox_next = page.next_offset;
        self.inbox_loaded = true;
        changed
    }
    // Presentation position maps back to the original suggestion index; custom is last.
    fn choices(&self) -> Vec<usize> {
        let Some(pending) = self.current() else {
            return Vec::new();
        };
        let mut indices = Vec::with_capacity(pending.request.options.len());
        if let Some(index) = pending.request.recommendation {
            indices.push(index);
        }
        indices.extend(
            (0..pending.request.options.len())
                .filter(|index| Some(*index) != pending.request.recommendation),
        );
        indices
    }
    fn key(&mut self, key: KeyEvent) -> Action {
        if key.kind != KeyEventKind::Press {
            return Action::None;
        }
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return Action::Quit;
        }
        if self.help {
            if matches!(key.code, KeyCode::Esc | KeyCode::Char('?'))
                || (key.code == KeyCode::Char('g') && key.modifiers.contains(KeyModifiers::CONTROL))
            {
                self.help = false;
                return Action::Redraw;
            }
            match key.code {
                KeyCode::Char('j') | KeyCode::Down | KeyCode::Right => {
                    self.help_page = (self.help_page + 1).min(30)
                }
                KeyCode::Char('k') | KeyCode::Up | KeyCode::Left => {
                    self.help_page = self.help_page.saturating_sub(1)
                }
                _ => return Action::None,
            }
            return Action::Redraw;
        }
        if (key.code == KeyCode::Char('g') && key.modifiers.contains(KeyModifiers::CONTROL))
            || (key.code == KeyCode::Char('?')
                && self.composer.is_none()
                && !self.editing
                && self.history_lookup.is_none()
                && !self.search.as_ref().is_some_and(|search| search.editing))
        {
            self.help = true;
            self.help_page = 0;
            return Action::Redraw;
        }
        if let Some(composer) = &mut self.composer {
            if composer.reply_to.is_none() {
                if key.code == KeyCode::Char('o') && key.modifiers.contains(KeyModifiers::CONTROL) {
                    self.suspended_instruction = self.composer.take();
                    return Action::Redraw;
                }
                if composer.confirm_discard {
                    if key.code == KeyCode::Char('y') {
                        self.composer = None;
                    } else {
                        composer.confirm_discard = false;
                    }
                    return Action::Redraw;
                }
                match key.code {
                    KeyCode::Esc if composer.error.is_some() => composer.confirm_discard = true,
                    KeyCode::Esc => self.composer = None,
                    KeyCode::Enter => {
                        if let Some(message) = composer.message() {
                            return Action::SendInstruction(message);
                        }
                    }
                    KeyCode::Left => composer.cursor = composer.cursor.saturating_sub(1),
                    KeyCode::Right => {
                        composer.cursor =
                            (composer.cursor + 1).min(composer.fields[3].chars().count())
                    }
                    KeyCode::Home => composer.cursor = 0,
                    KeyCode::End => composer.cursor = composer.fields[3].chars().count(),
                    KeyCode::Backspace if composer.error.is_none() && composer.cursor > 0 => {
                        let start = composer.fields[3]
                            .char_indices()
                            .nth(composer.cursor - 1)
                            .map(|(i, _)| i)
                            .unwrap_or(0);
                        let end = composer.fields[3]
                            .char_indices()
                            .nth(composer.cursor)
                            .map(|(i, _)| i)
                            .unwrap_or(composer.fields[3].len());
                        composer.fields[3].replace_range(start..end, "");
                        composer.cursor -= 1;
                    }
                    KeyCode::Char(c)
                        if composer.error.is_none()
                            && !c.is_control()
                            && !key
                                .modifiers
                                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                    {
                        let at = composer.fields[3]
                            .char_indices()
                            .nth(composer.cursor)
                            .map(|(i, _)| i)
                            .unwrap_or(composer.fields[3].len());
                        composer.fields[3].insert(at, c);
                        composer.cursor += 1;
                    }
                    _ => {}
                }
                return Action::Redraw;
            }
            match key.code {
                KeyCode::Esc => {
                    self.composer = None;
                    return Action::Redraw;
                }
                KeyCode::Char('j') | KeyCode::Down | KeyCode::Right
                    if composer.stage == composer.review_stage() =>
                {
                    let last = composer_pages(composer).len().saturating_sub(1);
                    composer.review_page = (composer.review_page + 1).min(last);
                }
                KeyCode::Char('k') | KeyCode::Up | KeyCode::Left
                    if composer.stage == composer.review_stage() =>
                {
                    composer.review_page = composer.review_page.saturating_sub(1);
                }
                KeyCode::Enter if composer.stage == composer.review_stage() => {
                    if composer.reply_to.is_some() {
                        if let Some(reply) = composer.reply() {
                            return Action::SendReply(reply);
                        }
                    } else if let Some(message) = composer.message() {
                        return Action::SendInstruction(message);
                    }
                }
                KeyCode::Enter => {
                    let valid = composer
                        .fields
                        .get(composer.editing_field())
                        .is_some_and(|field| !field.is_empty());
                    if valid {
                        composer.stage += 1;
                        composer.error = None;
                    }
                }
                KeyCode::Backspace if composer.stage < composer.review_stage() => {
                    let index = composer.editing_field();
                    if let Some(field) = composer.fields.get_mut(index) {
                        field.pop();
                    }
                }
                KeyCode::Char(c)
                    if composer.stage < composer.review_stage()
                        && !c.is_control()
                        && !key
                            .modifiers
                            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                {
                    let index = composer.editing_field();
                    if let Some(field) = composer.fields.get_mut(index) {
                        field.push(c);
                    }
                }
                _ => {}
            }
            return Action::Redraw;
        }
        if let Some(lookup) = &mut self.history_lookup {
            match key.code {
                KeyCode::Esc => self.history_lookup = None,
                KeyCode::Char('d') if lookup.kind.is_none() => {
                    lookup.kind = Some(HistoryKind::Decision)
                }
                KeyCode::Char('m') if lookup.kind.is_none() => {
                    lookup.kind = Some(HistoryKind::Message)
                }
                KeyCode::Char(c @ '0'..='9') if lookup.kind.is_some() => {
                    lookup.id.push(c);
                    lookup.invalid = false;
                }
                KeyCode::Backspace if lookup.kind.is_some() => {
                    lookup.id.pop();
                    lookup.invalid = false;
                }
                KeyCode::Enter => {
                    if let (Some(kind), Ok(id)) = (lookup.kind, lookup.id.parse::<i64>()) {
                        if id > 0 {
                            return Action::OpenHistory(kind, id);
                        }
                    }
                    lookup.invalid = true;
                }
                _ => return Action::None,
            }
            return Action::Redraw;
        }
        if let Some(history) = &mut self.history {
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc | KeyCode::Char('H') => {
                    return Action::CloseHistory
                }
                KeyCode::Char(c @ '1'..='3') => {
                    history.section = (c as usize) - ('1' as usize);
                    history.offset = 0;
                    history.previous.clear();
                    history.reload();
                    return Action::LoadHistory;
                }
                KeyCode::Char('R') => {
                    history.loading = true;
                    history.needs_load = true;
                    history.error = None;
                    return Action::LoadHistory;
                }
                KeyCode::Home => history.scroll.set(0),
                KeyCode::End => history.scroll.set(history.max_scroll.get()),
                KeyCode::Char('j') | KeyCode::Down | KeyCode::PageDown | KeyCode::Char(']') => {
                    let current = history.scroll.get();
                    let max = history.max_scroll.get();
                    if (current >= max || key.code == KeyCode::Char(']'))
                        && history.next_offset.is_some()
                        && !history.loading
                    {
                        history.previous.push(history.offset);
                        history.offset = history.next_offset.unwrap_or(history.offset);
                        history.items.clear();
                        history.scroll.set(0);
                        history.anchor.set((0, 0, 0));
                        history.loading = true;
                        history.needs_load = true;
                        history.error = None;
                        return Action::LoadHistory;
                    }
                    let step = if key.code == KeyCode::PageDown {
                        usize::from(
                            terminal::size()
                                .map(|(_, h)| h)
                                .unwrap_or(24)
                                .saturating_sub(3),
                        )
                        .max(1)
                    } else {
                        1
                    };
                    history.scroll.set(current.saturating_add(step).min(max));
                }
                KeyCode::Char('k') | KeyCode::Up | KeyCode::PageUp | KeyCode::Char('[') => {
                    let current = history.scroll.get();
                    if (current == 0 || key.code == KeyCode::Char('['))
                        && !history.previous.is_empty()
                        && !history.loading
                    {
                        history.offset = history.previous.pop().unwrap_or(0);
                        history.items.clear();
                        history.scroll.set(usize::MAX);
                        history.anchor.set((0, 0, 0));
                        history.loading = true;
                        history.needs_load = true;
                        history.error = None;
                        return Action::LoadHistory;
                    }
                    let step = if key.code == KeyCode::PageUp {
                        usize::from(
                            terminal::size()
                                .map(|(_, h)| h)
                                .unwrap_or(24)
                                .saturating_sub(3),
                        )
                        .max(1)
                    } else {
                        1
                    };
                    history.scroll.set(current.saturating_sub(step));
                }
                _ => return Action::None,
            }
            return Action::Redraw;
        }
        if let Some(overview) = &mut self.overview {
            return match key.code {
                KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('v') => {
                    self.overview = None;
                    Action::CloseOverview
                }
                KeyCode::Char('R') if !overview.loading => {
                    overview.dirty = true;
                    Action::LoadOverview
                }
                _ => Action::None,
            };
        }
        if let Some(search) = &mut self.search {
            if key.code == KeyCode::Esc {
                self.search = None;
                return Action::CloseSearch;
            }
            if search.loading {
                return Action::None;
            }
            if search.editing {
                match key.code {
                    KeyCode::Enter if !search.query.trim().is_empty() => {
                        search.after = 0;
                        search.previous.clear();
                        search.editing = false;
                        search.loading = true;
                        search.error = None;
                        return Action::LoadSearch(search.query.clone(), 0);
                    }
                    KeyCode::Backspace => {
                        search.query.pop();
                    }
                    KeyCode::Char(c)
                        if !c.is_control()
                            && !key
                                .modifiers
                                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
                            && search.query.chars().count() < 256 =>
                    {
                        search.query.push(c)
                    }
                    _ => return Action::None,
                }
            } else {
                match key.code {
                    KeyCode::Char('e') => {
                        search.editing = true;
                    }
                    KeyCode::Char('R') if search.error.is_some() => {
                        search.loading = true;
                        return Action::LoadSearch(search.query.clone(), search.after);
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        let len = search.page.as_ref().map_or(0, |page| page.messages.len());
                        search.selected =
                            search.selected.saturating_add(1).min(len.saturating_sub(1));
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        search.selected = search.selected.saturating_sub(1)
                    }
                    KeyCode::Char('n') if search.error.is_none() => {
                        if let Some(next) = search.page.as_ref().and_then(|page| page.next_after) {
                            search.previous.push(search.after);
                            search.after = next;
                            search.selected = 0;
                            search.loading = true;
                            return Action::LoadSearch(search.query.clone(), next);
                        }
                    }
                    KeyCode::Char('p') => {
                        if let Some(previous) = search.previous.pop() {
                            search.after = previous;
                            search.selected = 0;
                            search.loading = true;
                            return Action::LoadSearch(search.query.clone(), previous);
                        }
                    }
                    KeyCode::Enter if search.error.is_none() => {
                        if let Some(item) = search
                            .page
                            .as_ref()
                            .and_then(|page| page.messages.get(search.selected))
                        {
                            return Action::OpenHistory(HistoryKind::Message, item.id);
                        }
                    }
                    _ => return Action::None,
                }
            }
            return Action::Redraw;
        }
        if key.code == KeyCode::Char('o')
            && key.modifiers.contains(KeyModifiers::CONTROL)
            && !self.editing
        {
            if let Some(draft) = self.suspended_answer.take() {
                self.inbox_view = false;
                self.notice_view = false;
                self.requests = vec![draft.request];
                self.current = 0;
                self.request_offset = draft.offset;
                self.request_previous = draft.previous;
                self.request_next = None;
                self.request_select_last = None;
                self.request_loaded = true;
                self.selected = Some(self.choices().len());
                self.custom = draft.text;
                self.editing = true;
                self.list_focus = false;
                self.detail_page = None;
                self.detail_option = None;
                self.priority_edit = false;
                self.follow_answer = draft.follow_answer;
                self.detail_scroll.set(draft.scroll);
                self.detail_anchor.set(draft.anchor);
                self.draft_request_missing = true;
                if self.status.as_deref() == Some("Answer draft held; Ctrl-O resumes it") {
                    self.status = None;
                }
                return Action::ReloadRequests;
            }
        }
        if key.code == KeyCode::Char('v')
            && !self.editing
            && !self.priority_edit
            && self.detail_page.is_none()
        {
            self.overview = Some(OverviewView {
                dirty: true,
                ..OverviewView::default()
            });
            return Action::LoadOverview;
        }
        if key.code == KeyCode::Char('/')
            && !self.editing
            && !self.priority_edit
            && self.detail_page.is_none()
        {
            self.search = Some(SearchView {
                editing: true,
                ..SearchView::default()
            });
            return Action::Redraw;
        }
        if key.code == KeyCode::Char('R')
            && self.poll_error.is_some()
            && !self.editing
            && !self.priority_edit
            && self.detail_page.is_none()
        {
            return Action::ReloadRequests;
        }
        let mut key = key;
        let queue_view = self.inbox_view
            || self.notice_view
            || (self.detail_page.is_none() && !self.editing && !self.priority_edit);
        if queue_view && key.code == KeyCode::Tab {
            self.list_focus = !self.list_focus;
            return Action::Redraw;
        }
        if queue_view && self.list_focus {
            key.code = match key.code {
                KeyCode::Char('j') | KeyCode::Down => KeyCode::Right,
                KeyCode::Char('k') | KeyCode::Up => KeyCode::Left,
                other => other,
            };
            if !self.inbox_view
                && !self.notice_view
                && matches!(key.code, KeyCode::Enter | KeyCode::Char('1'..='9'))
            {
                return Action::None;
            }
        }
        if queue_view
            && matches!(
                key.code,
                KeyCode::Left | KeyCode::Right | KeyCode::Char('h' | 'l')
            )
        {
            if self.inbox_view {
                self.inbox_scroll.set(0);
                self.inbox_anchor.set((0, 0, 0));
            } else if self.notice_view {
                self.notice_scroll.set(0);
                self.notice_anchor.set((0, 0, 0));
            } else {
                self.detail_scroll.set(0);
                self.detail_anchor.set((0, 0, 0));
                self.follow_answer = true;
            }
        }
        if queue_view && !self.list_focus && matches!(key.code, KeyCode::PageUp | KeyCode::PageDown)
        {
            let step = usize::from(
                terminal::size()
                    .map(|(_, h)| h)
                    .unwrap_or(24)
                    .saturating_sub(6),
            )
            .max(1);
            let current = self.detail_actual.get();
            let next = if key.code == KeyCode::PageUp {
                current.saturating_sub(step)
            } else {
                current.saturating_add(step).min(self.detail_max.get())
            };
            if self.inbox_view {
                self.inbox_scroll.set(next);
            } else if self.notice_view {
                self.notice_scroll.set(next);
            } else {
                self.detail_scroll.set(next);
                self.follow_answer = false;
            }
            return Action::Redraw;
        }
        if self.inbox_view {
            match key.code {
                KeyCode::Char('i') | KeyCode::Char('q') | KeyCode::Esc => {
                    return Action::ToggleInbox
                }
                KeyCode::Char('g') => {
                    self.history_lookup = Some(HistoryLookup::default());
                }
                KeyCode::Char('H') if self.inbox_loaded => {
                    if let Some(item) = self.inbox.get(self.inbox_current) {
                        return Action::OpenHistory(HistoryKind::Message, item.id);
                    }
                }
                KeyCode::Char('r') if self.inbox_loaded && self.suspended_instruction.is_none() => {
                    if let Some(item) = self.inbox.get(self.inbox_current) {
                        if item.message.expects_reply
                            && matches!(&item.message.target, Target::Participant { name } if name == "operator")
                        {
                            let mut composer = Composer {
                                reply_to: Some(item.id),
                                ..Composer::default()
                            };
                            if let Some(target) = composer.fields.get_mut(2) {
                                *target = item.message.sender.clone();
                            }
                            self.composer = Some(composer);
                            self.status = None;
                        }
                    }
                }
                KeyCode::Char('h') | KeyCode::Left if self.inbox_loaded => {
                    if self.inbox_current == 0 {
                        if let Some(previous) = self.inbox_previous.pop() {
                            self.inbox_offset = previous;
                            self.inbox_loaded = false;
                            self.inbox_select_last = Some(true);
                            self.inbox_page = 0;
                            return Action::ReloadInbox;
                        }
                    } else {
                        self.inbox_current -= 1;
                        self.inbox_page = 0;
                    }
                }
                KeyCode::Char('l') | KeyCode::Right if self.inbox_loaded => {
                    if self.inbox_current + 1 < self.inbox.len() {
                        self.inbox_current += 1;
                        self.inbox_page = 0;
                    } else if let Some(next) = self.inbox_next {
                        self.inbox_previous.push(self.inbox_offset);
                        self.inbox_offset = next;
                        self.inbox_loaded = false;
                        self.inbox_select_last = Some(false);
                        self.inbox_page = 0;
                        return Action::ReloadInbox;
                    }
                }
                KeyCode::Char('j') | KeyCode::Down => {
                    self.inbox_scroll.set(
                        self.detail_actual
                            .get()
                            .saturating_add(1)
                            .min(self.detail_max.get()),
                    );
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.inbox_scroll
                        .set(self.detail_actual.get().saturating_sub(1));
                }
                _ => return Action::None,
            }
            return Action::Redraw;
        }
        if self.notice_view {
            match key.code {
                KeyCode::Char('n') | KeyCode::Char('q') | KeyCode::Esc => {
                    return Action::ToggleNotices
                }
                KeyCode::Char('h') | KeyCode::Left if self.notice_loaded => {
                    if self.notice_current == 0 {
                        if let Some(previous) = self.notice_previous.pop() {
                            self.notice_offset = previous;
                            self.notice_loaded = false;
                            self.notice_select_last = Some(true);
                            self.notice_page = 0;
                            return Action::ReloadNotices;
                        }
                    } else {
                        self.notice_current -= 1;
                        self.notice_page = 0;
                    }
                }
                KeyCode::Char('l') | KeyCode::Right if self.notice_loaded => {
                    if self.notice_current + 1 < self.notices.len() {
                        self.notice_current += 1;
                        self.notice_page = 0;
                    } else if let Some(next) = self.notice_next {
                        self.notice_previous.push(self.notice_offset);
                        self.notice_offset = next;
                        self.notice_loaded = false;
                        self.notice_select_last = Some(false);
                        self.notice_page = 0;
                        return Action::ReloadNotices;
                    }
                }
                KeyCode::Char('j') | KeyCode::Down if self.notice_loaded => {
                    self.notice_scroll.set(
                        self.detail_actual
                            .get()
                            .saturating_add(1)
                            .min(self.detail_max.get()),
                    );
                }
                KeyCode::Char('k') | KeyCode::Up if self.notice_loaded => {
                    self.notice_scroll
                        .set(self.detail_actual.get().saturating_sub(1));
                }
                KeyCode::Char('d') if self.notice_loaded => {
                    if let Some(notice) = self.notices.get(self.notice_current) {
                        return Action::Submit(Operation::DismissNotice { id: notice.id });
                    }
                }
                _ => return Action::None,
            }
            return Action::Redraw;
        }
        if let Some(page) = self.detail_page {
            match key.code {
                KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('c') | KeyCode::Char('o') => {
                    self.detail_page = None
                }
                KeyCode::Char('j') | KeyCode::Down | KeyCode::Right => {
                    let max_page = self
                        .current()
                        .map(|r| {
                            detail_pages(&r.request, self.detail_option)
                                .len()
                                .saturating_sub(1)
                        })
                        .unwrap_or(0);
                    self.detail_page = Some((page + 1).min(max_page));
                }
                KeyCode::Char('k') | KeyCode::Up | KeyCode::Left => {
                    self.detail_page = Some(page.saturating_sub(1))
                }
                _ => return Action::None,
            }
            return Action::Redraw;
        }
        if self.priority_edit {
            match key.code {
                KeyCode::Esc => {
                    self.priority_edit = false;
                    return Action::Redraw;
                }
                KeyCode::Char(c @ '0'..='5') => {
                    if let Some(request) = self.current() {
                        return Action::Submit(Operation::SetImportance {
                            id: request.id,
                            importance: c as u8 - b'0',
                        });
                    }
                }
                _ => {}
            }
            return Action::None;
        }
        if self.editing {
            match key.code {
                KeyCode::Char('o') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    if let Some(request) = self.current().cloned() {
                        self.suspended_answer = Some(SuspendedAnswer {
                            request,
                            text: std::mem::take(&mut self.custom),
                            offset: self.request_offset,
                            previous: self.request_previous.clone(),
                            scroll: self.detail_scroll.get(),
                            anchor: self.detail_anchor.get(),
                            follow_answer: self.follow_answer,
                        });
                        self.editing = false;
                        if self.draft_request_missing {
                            return Action::ReloadRequests;
                        }
                    }
                }
                KeyCode::Esc => {
                    self.editing = false;
                    if self.draft_request_missing {
                        return Action::ReloadRequests;
                    }
                }
                KeyCode::Backspace => {
                    self.custom.pop();
                }
                KeyCode::Enter if !self.custom.is_empty() => {
                    if let Some(request) = self.current() {
                        return Action::Submit(Operation::AnswerCustom {
                            id: request.id,
                            value: self.custom.clone(),
                        });
                    }
                }
                KeyCode::Char(c)
                    if !c.is_control()
                        && !key
                            .modifiers
                            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                {
                    self.custom.push(c)
                }
                _ => {}
            }
            return Action::Redraw;
        }
        if matches!(key.code, KeyCode::Char('q') | KeyCode::Esc)
            && self
                .suspended_instruction
                .as_ref()
                .is_some_and(|draft| draft.error.is_some())
        {
            self.composer = self.suspended_instruction.take();
            if let Some(composer) = &mut self.composer {
                composer.confirm_discard = true;
            }
            return Action::Redraw;
        }
        if key.code == KeyCode::Char('g') {
            self.history_lookup = Some(HistoryLookup::default());
            return Action::Redraw;
        }
        if key.code == KeyCode::Char('i') {
            return Action::ToggleInbox;
        }
        if key.code == KeyCode::Char('m') {
            if let Some(draft) = self.suspended_instruction.take() {
                self.composer = Some(draft);
                return Action::Redraw;
            }
            match Composer::instruction() {
                Ok(composer) => {
                    self.composer = Some(composer);
                    self.status = None;
                    self.status_expires = None;
                }
                Err(error) => {
                    self.status_expires = None;
                    self.status = Some(format!("Cannot create instruction identity: {error}"))
                }
            }
            return Action::Redraw;
        }
        if key.code == KeyCode::Char('n') {
            return Action::ToggleNotices;
        }
        if key.code == KeyCode::Char('U') {
            if let Some((id, identity)) = &self.defer_retry {
                return Action::Submit(Operation::DeferDecision {
                    id: *id,
                    identity: identity.clone(),
                });
            }
        }
        if !self.request_loaded {
            return if key.code == KeyCode::Char('q') {
                Action::Quit
            } else {
                Action::None
            };
        }
        if key.code == KeyCode::Enter && !self.list_focus && !self.follow_answer {
            self.follow_answer = true;
            return Action::Redraw;
        }
        let choices = self.choices();
        let Some(request) = self.current() else {
            return if key.code == KeyCode::Char('q') {
                Action::Quit
            } else {
                Action::None
            };
        };
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => Action::Quit,
            KeyCode::Char('h') | KeyCode::Left => {
                if !self.request_loaded {
                    return Action::None;
                }
                if self.current == 0 {
                    if let Some(previous) = self.request_previous.pop() {
                        self.request_offset = previous;
                        self.request_loaded = false;
                        self.request_select_last = Some(true);
                        return Action::ReloadRequests;
                    }
                    return Action::None;
                }
                self.current -= 1;
                self.reset_selection();
                Action::Redraw
            }
            KeyCode::Char('l') | KeyCode::Right => {
                if !self.request_loaded {
                    return Action::None;
                }
                if self.current + 1 >= self.requests.len() {
                    if let Some(next) = self.request_next {
                        self.request_previous.push(self.request_offset);
                        self.request_offset = next;
                        self.request_loaded = false;
                        self.request_select_last = Some(false);
                        return Action::ReloadRequests;
                    }
                    return Action::None;
                }
                self.current += 1;
                self.reset_selection();
                Action::Redraw
            }
            KeyCode::Char('H') => Action::OpenHistory(HistoryKind::Decision, request.id),
            KeyCode::Char('d') => {
                let id = request.id;
                let identity = match &self.defer_retry {
                    Some((pending, identity)) if *pending == id => identity.clone(),
                    Some(_) => {
                        self.status =
                            Some("Unconfirmed deferral: U retries the previous action".into());
                        return Action::Redraw;
                    }
                    None => match Composer::instruction() {
                        Ok(composer) => composer.fields[0].clone(),
                        Err(error) => {
                            self.status = Some(format!("Cannot create deferral identity: {error}"));
                            return Action::Redraw;
                        }
                    },
                };
                self.defer_retry = Some((id, identity.clone()));
                Action::Submit(Operation::DeferDecision { id, identity })
            }
            KeyCode::Char('c') => {
                self.detail_option = None;
                self.detail_page = Some(0);
                Action::Redraw
            }
            KeyCode::Char('o') => {
                if let Some(index) = self
                    .selected
                    .and_then(|position| choices.get(position))
                    .copied()
                {
                    self.detail_option = Some(index);
                    self.detail_page = Some(0);
                    Action::Redraw
                } else {
                    Action::None
                }
            }
            KeyCode::Char('p') => {
                self.priority_edit = true;
                Action::Redraw
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.follow_answer = true;
                self.selected = Some(self.selected.map_or(0, |i| (i + 1).min(choices.len())));
                Action::Redraw
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.follow_answer = true;
                self.selected = Some(self.selected.map_or(0, |i| i.saturating_sub(1)));
                Action::Redraw
            }
            KeyCode::Char(c @ '1'..='9') => {
                let position = c as usize - '1' as usize;
                if position <= choices.len() {
                    self.follow_answer = true;
                    self.selected = Some(position);
                    Action::Redraw
                } else {
                    Action::None
                }
            }
            KeyCode::Enter => match self.selected {
                Some(position) if position == choices.len() => {
                    if self.suspended_answer.is_some() {
                        self.status = Some("Answer draft held; Ctrl-O resumes it".into());
                        self.status_expires = None;
                    } else {
                        self.editing = true;
                    }
                    Action::Redraw
                }
                Some(position) => match choices.get(position) {
                    Some(index) => Action::Submit(Operation::Answer {
                        id: request.id,
                        index: *index,
                    }),
                    None => Action::None,
                },
                None => Action::None,
            },
            _ => Action::None,
        }
    }
}
enum Action {
    None,
    Redraw,
    Quit,
    ReloadRequests,
    ToggleInbox,
    ReloadInbox,
    ToggleNotices,
    ReloadNotices,
    OpenHistory(HistoryKind, i64),
    CloseHistory,
    LoadHistory,
    LoadSearch(String, i64),
    CloseSearch,
    LoadOverview,
    CloseOverview,
    SendInstruction(Message),
    SendReply(MessageReply),
    Submit(Operation),
}

pub async fn terminal(address: &str) -> Result<()> {
    // Register handlers before entering raw mode, so ordinary termination
    // signals return through the terminal guard instead of leaving raw mode set.
    let mut interrupt = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    terminal::enable_raw_mode()?;
    let mut events = EventStream::new();
    let (sender, mut receiver) = mpsc::channel(256);
    let input = tokio::spawn(async move {
        while let Some(event) = events.next().await {
            if sender.send(event).await.is_err() {
                break;
            }
        }
    });
    let _guard = TerminalGuard { input };
    let (_watch, mut changes) = network::watch_changes(address);
    let refresh_trigger = Notify::new();
    let overview_trigger = Notify::new();
    refresh_trigger.notify_one();
    execute!(
        io::stdout(),
        terminal::EnterAlternateScreen,
        EnableMouseCapture
    )?;
    let mut view = View::new();
    let mut screen = Terminal::new(CrosstermBackend::new(BasicColorWriter {
        inner: io::stdout(),
        pending: Vec::new(),
    }))?;
    screen.clear()?;
    let mut generation = 0u64;
    let mut refresh: Option<tokio::task::JoinHandle<_>> = None;
    let mut submission: Option<
        tokio::task::JoinHandle<(bool, bool, bool, Result<serde_json::Value>)>,
    > = None;
    let mut history_task: Option<tokio::task::JoinHandle<_>> = None;
    let mut search_task: Option<tokio::task::JoinHandle<Result<SearchPage>>> = None;
    let mut overview_task: Option<tokio::task::JoinHandle<Result<OverviewCounts>>> = None;
    display(&mut screen, &view)?;
    loop {
        let status_deadline = view.status_expires;
        tokio::select! {
            _ = async { if let Some(deadline) = status_deadline { time::sleep_until(deadline.into()).await } else { std::future::pending().await } }, if status_deadline.is_some() => {
                if view.status_expires.is_some_and(|deadline| Instant::now() >= deadline) {
                    view.status = None;
                    view.status_expires = None;
                    display(&mut screen, &view)?;
                }
            }
            _ = interrupt.recv() => return Ok(()),
            _ = terminate.recv() => return Ok(()),
            changed = changes.changed() => {
                if changed.is_err() { return Err(crate::error::AppError::Task("change subscription closed".into())); }
                let state = *changes.borrow_and_update();
                let was_unavailable = view.connection_unavailable;
                view.connection_unavailable = !state.connected;
                if let Some(overview) = &mut view.overview {
                    overview.dirty = true;
                    if state.connected { overview_trigger.notify_one(); }
                }
                if state.connected { refresh_trigger.notify_one(); }
                if was_unavailable != view.connection_unavailable { display(&mut screen, &view)?; }
            }
            _ = overview_trigger.notified(), if view.overview.as_ref().is_some_and(|overview| overview.dirty) && overview_task.is_none() => {
                if let Some(overview) = &mut view.overview {
                    overview.dirty = false;
                    overview.loading = true;
                    overview.error = None;
                    let address = address.to_owned();
                    overview_task = Some(tokio::spawn(async move { fetch_overview(&address).await }));
                    display(&mut screen, &view)?;
                }
            }
            _ = refresh_trigger.notified(), if !view.help && ((view.history.as_ref().is_some_and(|history| history.needs_load) && history_task.is_none()) || (view.history.is_none() && view.composer.is_none() && refresh.is_none() && submission.is_none())) => {
                if let Some(history) = &mut view.history {
                    history.needs_load = false;
                    let (kind, id, section, offset, root_first) = (history.kind, history.id, history.section, history.offset, history.root_first);
                    let address = address.to_owned();
                    let started = generation;
                    history_task = Some(tokio::spawn(async move { (started, fetch_history(&address, kind, id, section, offset, root_first).await) }));
                } else {
                    let address = address.to_owned();
                    let request_offset = view.request_offset;
                    let inbox_view = view.inbox_view;
                    let inbox_offset = view.inbox_offset;
                    let notice_offset = view.notice_offset;
                    let current_generation = generation;
                    refresh = Some(tokio::spawn(async move {
                        let result = tokio::join!(
                            fetch_requests(&address, request_offset),
                            fetch_notices(&address, notice_offset),
                            async { if inbox_view { Some(fetch_operator_inbox(&address, inbox_offset).await) } else { None } },
                        );
                        (current_generation, inbox_view, result)
                    }));
                }
            }
            result = async {
                match refresh.as_mut() {
                    Some(task) => task.await,
                    None => std::future::pending().await,
                }
            }, if refresh.is_some() => {
                refresh = None;
                let (started, inbox_view, refreshed) = result.map_err(|error| crate::error::AppError::Task(error.to_string()))?;
                // A refresh started before a queue switch or action must never
                // replace newer UI state with an obsolete snapshot.
                if started != generation || inbox_view != view.inbox_view || view.composer.is_some() || view.history.is_some() || view.busy || view.help {
                    refresh_trigger.notify_one();
                    continue;
                }
                let (requests, notices, messages) = refreshed;
                let requests_ok = requests.is_ok();
                let notices_ok = notices.is_ok();
                let messages_ok = messages.as_ref().is_some_and(|result| result.is_ok());
                let mut errors = Vec::new();
                let mut changed = false;
                match requests {
                    Ok(page) => changed |= view.update_request_page(page),
                    Err(error) => errors.push(("decisions", error.to_string())),
                }
                match notices {
                    Ok(page) => changed |= view.update_notices(page),
                    Err(error) => errors.push(("notices", error.to_string())),
                }
                if let Some(messages) = messages {
                    match messages {
                        Ok(page) => changed |= view.update_inbox(page),
                        Err(error) => errors.push(("inbox", error.to_string())),
                    }
                }
                let summary = if errors.is_empty() { None } else { Some(format!("Refresh: {}", errors.iter().map(|(name, _)| *name).collect::<Vec<_>>().join(", "))) };
                let error = if errors.is_empty() { None } else { Some(format!("Refresh incomplete: {}; press R to retry", errors.iter().map(|(name, error)| format!("{name}: {error}")).collect::<Vec<_>>().join("; "))) };
                if view.poll_summary != summary { view.poll_summary = summary; changed = true; }
                if view.poll_error != error { view.poll_error = error; changed = true; }
                if changed { display(&mut screen, &view)?; }
                // A successful page can reveal that an offset has moved beyond
                // the end. Load the selected fallback page without a timer.
                if (requests_ok && !view.request_loaded)
                    || (notices_ok && !view.notice_loaded)
                    || (messages_ok && !view.inbox_loaded) {
                    refresh_trigger.notify_one();
                }
            }
            result = async {
                match overview_task.as_mut() {
                    Some(task) => task.await,
                    None => std::future::pending().await,
                }
            }, if overview_task.is_some() => {
                overview_task = None;
                let result = result.map_err(|error| crate::error::AppError::Task(error.to_string()))?;
                if let Some(overview) = &mut view.overview {
                    overview.loading = false;
                    match result {
                        Ok(counts) => { overview.counts = Some(counts); overview.error = None; }
                        Err(error) => { overview.counts = None; overview.error = Some(error.to_string()); }
                    }
                    if overview.dirty && !view.connection_unavailable { overview_trigger.notify_one(); }
                    display(&mut screen, &view)?;
                }
            }
            result = async {
                match search_task.as_mut() {
                    Some(task) => task.await,
                    None => std::future::pending().await,
                }
            }, if search_task.is_some() => {
                search_task = None;
                let result = result.map_err(|error| crate::error::AppError::Task(error.to_string()))?;
                if let Some(search) = &mut view.search {
                    search.loading = false;
                    match result {
                        Ok(page) => { search.page = Some(page); search.error = None; }
                        Err(error) => { search.page = None; search.error = Some(error.to_string()); }
                    }
                    display(&mut screen, &view)?;
                }
            }
            result = async {
                match history_task.as_mut() {
                    Some(task) => task.await,
                    None => std::future::pending().await,
                }
            }, if history_task.is_some() => {
                history_task = None;
                let (started, result) = result.map_err(|error| crate::error::AppError::Task(error.to_string()))?;
                if started == generation {
                    if let Some(history) = &mut view.history {
                        history.loading = false;
                        match result {
                            Ok(page) => {
                                history.items = page.items;
                                history.next_offset = page.next_offset;
                                history.error = None;
                            }
                            Err(error) => { history.error = Some(error.to_string()); }
                        }
                        display(&mut screen, &view)?;
                    }
                }
                if view.history.as_ref().is_some_and(|history| history.needs_load) { refresh_trigger.notify_one(); }
            }
            result = async {
                match submission.as_mut() {
                    Some(task) => task.await,
                    None => std::future::pending().await,
                }
            }, if submission.is_some() => {
                submission = None;
                view.busy = false;
                view.status_expires = None;
                let (is_instruction, is_notice, is_defer, result) = result.map_err(|error| crate::error::AppError::Task(error.to_string()))?;
                if is_instruction {
                    match result {
                        Ok(_response) => {
                            let label = if view.composer.as_ref().is_some_and(|composer| composer.reply_to.is_some()) { "Reply" } else { "Instruction" };
                            view.composer = None;
                            view.status = Some(format!("{label} sent"));
                            view.status_expires = Some(Instant::now() + Duration::from_secs(4));
                        }
                        Err(error) => { if let Some(composer) = &mut view.composer {
                            composer.error = Some(match error {
                                crate::error::AppError::Remote(message) if matches!(message.as_str(), "invalid request" | "identity conflict" | "request not found" | "request already answered" | "invalid option") => {
                                    if composer.reply_to.is_none() && message == "invalid request" {
                                        format!("Rejected: {message}. Ensure channel all includes operator and at least one other member.")
                                    } else { format!("Rejected: {message}") }
                                },
                                other => format!("Unconfirmed: {other}; it may have been accepted."),
                            });
                        } }
                    }
                } else {
                    match result {
                        Ok(_response) => { if is_defer { view.defer_retry = None; } view.status = Some(if is_notice { "Notice dismissed".into() } else if is_defer { "Decision deferred; notice recorded".into() } else { "Action accepted".into() }); view.status_expires = Some(Instant::now() + Duration::from_secs(4)); if !is_notice { view.reset_selection(); view.reset_request_page(); } view.reset_notice_page(); }
                        Err(error) => { if is_defer && rejection(&error) { view.defer_retry = None; } view.status = Some(if rejection(&error) { format!("Rejected: {error}; inspect current state") } else if is_defer { format!("Unconfirmed deferral: {error}; U retries the same action") } else { format!("Unconfirmed: {error}; check state before retry") }); }
                    }
                }
                display(&mut screen, &view)?;
                refresh_trigger.notify_one();
            }
            event = receiver.recv() => {
                let Some(event) = event else { return Ok(()); };
                let action = match event? {
                    Event::Key(key) => {
                        let too_small = terminal::size().is_ok_and(|(w, h)| w < 40 || h < 12);
                        if too_small
                            && !(key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL)) {
                            continue;
                        }
                        if view.busy {
                            if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                                return Ok(());
                            }
                            continue;
                        }
                        view.key(key)
                    }
                    Event::Mouse(mouse) => {
                        if view.busy { continue; }
                        let (width, height) = terminal::size().unwrap_or((0, 0));
                        view.mouse(mouse, width, height)
                    }
                    Event::Resize(width, height) => { view.resize_pages(page_size_for(width, height)); display(&mut screen, &view)?; continue; },
                    _ => continue,
                };
                match action {
                    Action::None => {},
                    Action::Redraw => display(&mut screen, &view)?,
                    Action::Quit => return Ok(()),
                    Action::OpenHistory(kind, id) => {
                        generation = generation.wrapping_add(1);
                        view.history_lookup = None;
                        let mut history = HistoryView::new(kind, id);
                        if view.search.is_some() { history.section = 1; history.root_first = true; }
                        view.history = Some(history);
                        display(&mut screen, &view)?; refresh_trigger.notify_one();
                    },
                    Action::CloseHistory => {
                        generation = generation.wrapping_add(1);
                        view.history = None;
                        display(&mut screen, &view)?; refresh_trigger.notify_one();
                    },
                    Action::LoadHistory => {
                        generation = generation.wrapping_add(1);
                        display(&mut screen, &view)?; refresh_trigger.notify_one();
                    },
                    Action::LoadSearch(query, after) => {
                        let address = address.to_owned();
                        search_task = Some(tokio::spawn(async move { fetch_search(&address, query, after).await }));
                        display(&mut screen, &view)?;
                    },
                    Action::CloseSearch => {
                        if let Some(task) = search_task.take() { task.abort(); }
                        display(&mut screen, &view)?;
                        refresh_trigger.notify_one();
                    },
                    Action::LoadOverview => {
                        overview_trigger.notify_one();
                        display(&mut screen, &view)?;
                    },
                    Action::CloseOverview => {
                        if let Some(task) = overview_task.take() { task.abort(); }
                        display(&mut screen, &view)?;
                        refresh_trigger.notify_one();
                    },
                    Action::ToggleNotices => {
                        generation = generation.wrapping_add(1);
                        if view.inbox_view {
                            view.inbox_view = false;
                            view.notice_view = true;
                        } else {
                            view.notice_view = !view.notice_view;
                        }
                        view.reset_notice_page();
                        display(&mut screen, &view)?; refresh_trigger.notify_one();
                    },
                    Action::ReloadNotices => {
                        generation = generation.wrapping_add(1);
                        display(&mut screen, &view)?; refresh_trigger.notify_one();
                    },
                    Action::ToggleInbox => {
                        generation = generation.wrapping_add(1);
                        if view.notice_view {
                            view.notice_view = false;
                            view.inbox_view = true;
                        } else {
                            view.inbox_view = !view.inbox_view;
                        }
                        view.inbox_loaded = false;
                        view.inbox_page = 0;
                        display(&mut screen, &view)?;
                        refresh_trigger.notify_one();
                    },
                    Action::ReloadInbox => {
                        generation = generation.wrapping_add(1);
                        display(&mut screen, &view)?;
                        refresh_trigger.notify_one();
                    },
                    Action::ReloadRequests => {
                        generation = generation.wrapping_add(1);
                        display(&mut screen, &view)?;
                        refresh_trigger.notify_one();
                    },
                    Action::SendInstruction(message) => {
                        generation = generation.wrapping_add(1);
                        if let Some(composer) = &mut view.composer { composer.error = None; }
                        view.busy = true;
                        display(&mut screen, &view)?;
                        let address = address.to_owned();
                        submission = Some(tokio::spawn(async move {
                            (true, false, false, submit_checked(&address, &Operation::SendMessage { message }).await)
                        }));
                    },
                    Action::SendReply(reply) => {
                        generation = generation.wrapping_add(1);
                        if let Some(composer) = &mut view.composer { composer.error = None; }
                        view.busy = true;
                        display(&mut screen, &view)?;
                        let address = address.to_owned();
                        submission = Some(tokio::spawn(async move {
                            (true, false, false, submit_checked(&address, &Operation::ReplyMessage { reply }).await)
                        }));
                    },
                    Action::Submit(operation) => {
                        generation = generation.wrapping_add(1);
                        view.status = None;
                        view.busy = true;
                        display(&mut screen, &view)?;
                        let address = address.to_owned();
                        let is_notice = matches!(&operation, Operation::DismissNotice { .. });
                        let is_defer = matches!(&operation, Operation::DeferDecision { .. });
                        submission = Some(tokio::spawn(async move {
                            (false, is_notice, is_defer, submit_checked(&address, &operation).await)
                        }));
                    }
                }
            }
        }
    }
}

// Restrict preview lines conservatively to avoid wrapping wide characters.
// Full question and answer text remain available in their detail views.
fn preview(text: &str) -> String {
    let (width, _) = terminal::size().unwrap_or((80, 24));
    let width = if width == 0 { 80 } else { width };
    let limit = (usize::from(width.saturating_sub(1)) / 2).max(1);
    let clean = safe(text);
    if clean.chars().count() <= limit {
        return clean;
    }
    let mut result: String = clean.chars().take(limit.saturating_sub(1)).collect();
    result.push('…');
    result
}

fn preview_tail(text: &str) -> String {
    let clean = safe(text);
    let (width, _) = terminal::size().unwrap_or((80, 24));
    let width = if width == 0 { 80 } else { width };
    let limit = (usize::from(width.saturating_sub(1)) / 2).max(1);
    if clean.chars().count() <= limit {
        return clean;
    }
    let tail: String = clean
        .chars()
        .rev()
        .take(limit.saturating_sub(1))
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("…{tail}")
}

// Do not let remote content inject terminal control sequences.
fn safe(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}
// Reserve rows for headers and controls, and allow two terminal columns per
// character so ordinary wide characters do not crowd the controls off screen.
fn detail_page_size() -> usize {
    let (width, height) = terminal::size().unwrap_or((80, 24));
    page_size_for(width, height)
}
fn page_size_for(width: u16, height: u16) -> usize {
    let width = if width == 0 { 80 } else { width };
    let height = if height == 0 { 24 } else { height };
    (usize::from(width.saturating_sub(1)) / 2)
        .max(1)
        .saturating_mul(usize::from(height.saturating_sub(6)).max(1))
        .clamp(1, 200)
}
fn detail_pages(request: &crate::protocol::DecisionRequest, option: Option<usize>) -> Vec<String> {
    let explanation = request.explanation.as_deref().unwrap_or("(none supplied)");
    let context = request.context.as_deref().unwrap_or("(none supplied)");
    let content = if let Some(index) = option {
        let answer = request.options.get(index).map(String::as_str).unwrap_or("");
        let reason = if request.recommendation == Some(index) {
            request.reason.as_deref().unwrap_or("")
        } else {
            ""
        };
        format!(
            "Answer: {}\nRecommendation reason: {}",
            safe(answer),
            safe(reason)
        )
    } else {
        format!(
            "Agent: {}\nQuestion: {}\nExplanation: {}\nSupporting context: {}",
            safe(&request.agent),
            safe(&request.question),
            safe(explanation),
            safe(context)
        )
    };
    let chars: Vec<char> = content.chars().collect();
    chars
        .chunks(detail_page_size())
        .map(|part| part.iter().collect())
        .collect()
}
fn text_pages(text: &str) -> Vec<String> {
    let chars: Vec<char> = safe(text).chars().collect();
    if chars.is_empty() {
        return vec![String::new()];
    }
    chars
        .chunks(detail_page_size())
        .map(|part| part.iter().collect())
        .collect()
}
fn composer_pages(composer: &Composer) -> Vec<String> {
    let [identity, channel, target, body] = &composer.fields;
    if let Some(request_id) = composer.reply_to {
        return text_pages(&format!(
            "Request: {request_id}   To: {}   Identity: {}   Reply: {}",
            safe(target),
            safe(identity),
            safe(channel)
        ));
    }
    let content = format!(
        "Identity: {}   Channel: {}   To: {}   Text: {}",
        safe(identity),
        safe(channel),
        if target.is_empty() {
            "(group)".into()
        } else {
            safe(target)
        },
        safe(body)
    );
    let chars: Vec<char> = content.chars().collect();
    chars
        .chunks(detail_page_size())
        .map(|part| part.iter().collect())
        .collect()
}
fn inbox_pages(item: &InboxItem) -> Vec<String> {
    let message = &item.message;
    let content = format!(
        "From: {}   Work context: {}   Message: {}   Activity: {}",
        safe(&message.sender),
        safe(message.context.as_deref().unwrap_or("(none supplied)")),
        safe(&message.body),
        safe(message.activity.as_deref().unwrap_or("(none supplied)"))
    );
    text_pages(&content)
}
fn notice_pages(item: &NoticeItem) -> Vec<String> {
    let content = format!(
        "Agent: {}   Work: {}   Result: {}   Activity: {}",
        safe(&item.notice.agent),
        safe(&item.notice.work),
        safe(&item.notice.result),
        safe(item.notice.activity.as_deref().unwrap_or("(none supplied)"))
    );
    let chars: Vec<char> = content.chars().collect();
    chars
        .chunks(detail_page_size())
        .map(|part| part.iter().collect())
        .collect()
}
fn display(screen: &mut Screen, view: &View) -> Result<()> {
    use std::fmt::Write as _;
    let mut output = String::new();
    macro_rules! println {
        ($($arg:tt)*) => {{ let _ = writeln!(&mut output, $($arg)*); }};
    }
    if view.help {
        let help = "Decisions: h/l request; j/k or 1-9 choose; Enter confirms
d defer; U retry unconfirmed deferral; p importance; c context; o answer detail
Tab focus; d defers to queue bottom
i inbox; n notices; m instruction
v overview
/ search retained messages
Inbox: Tab focus; arrows move; r reply; H history
Notices: Tab focus; arrows move; d dismiss
History: H selected; g lookup; 1-3 section
↑↓ scroll; PgUp/PgDn; R refresh; q returns
Instruction: Enter sends; arrows move
Reply: Enter continues/confirms; Esc cancels
Custom: Enter submits; Ctrl-O suspends/resumes; Esc cancels
? opens help outside text entry
Ctrl-G opens help anywhere; Ctrl-C quits";
        screen.draw(|frame| {
            let area = frame.area();
            let lines = wrap_visible(help, usize::from(area.width).max(1));
            let height = usize::from(area.height.saturating_sub(2));
            let scroll = view.help_page.min(lines.len().saturating_sub(height));
            let mut content = vec![
                Line::from("Keyboard help · ↑↓ scroll · Esc closes"),
                Line::from(""),
            ];
            content.extend(
                lines
                    .iter()
                    .skip(scroll)
                    .take(height)
                    .map(|(line, _)| Line::from(line.as_str())),
            );
            frame.render_widget(Paragraph::new(content), area);
        })?;
        return Ok(());
    }
    if view.busy {
        println!("Action in progress; outcome unconfirmed. Ctrl-C quits.\r");
    }
    if let Some(error) = &view.poll_error {
        let width = terminal::size().map(|(cols, _)| cols).unwrap_or(80);
        let text = if width > 0 && width < 60 {
            view.poll_summary.as_deref().unwrap_or(error)
        } else {
            error
        };
        println!("{}\r", safe(text));
    }
    if view.suspended_instruction.is_some() {
        println!("Instruction draft suspended; m resumes from decisions.\r");
    }
    if let Some(composer) = &view.composer {
        if composer.reply_to.is_none() {
            if composer.confirm_discard {
                println!("Discard this uncertain instruction?\r\nDestination: all · group\r\nID: {}\r\ny discards; any other key keeps it.", safe(&composer.fields[0]));
                return render(screen, &output);
            }
            let body = &composer.fields[3];
            let (width, _) = terminal::size().unwrap_or((80, 24));
            let capacity = usize::from(width.saturating_sub(4)).max(1) / 2;
            let start = composer.cursor.saturating_sub(capacity.saturating_sub(1));
            let visible: String = body.chars().skip(start).take(capacity).collect();
            println!("Compose instruction\r\nDestination: all · group\r\nSend to other channel members\r\n{}{}{}\r\n{}^\r\nEnter sends; ←/→ moves; Ctrl-O suspends; Esc discards.",
                if start > 0 { "…" } else { "" }, safe(&visible),
                if start + capacity < body.chars().count() { "…" } else { "" },
                " ".repeat(composer.cursor.saturating_sub(start) + usize::from(start > 0)));
            if view.busy {
                println!("Sending — outcome unconfirmed.");
            }
            if let Some(error) = &composer.error {
                println!(
                    "{}\r\nDraft locked. Enter retries unchanged; Esc discards.",
                    safe(error)
                );
            }
            return render(screen, &output);
        }
        let labels = ["Instruction text"];
        if composer.stage < composer.review_stage() {
            let reply_labels = ["Retry identity (unique deployment-wide)", "Reply text"];
            let (title, labels) = if composer.reply_to.is_some() {
                ("Compose reply", reply_labels.as_slice())
            } else {
                ("Compose instruction", labels.as_slice())
            };
            println!(
                "{}: {}\r\n{}\r\n\r\nEnter continues; Esc cancels; Ctrl-G help.",
                title,
                labels.get(composer.stage).unwrap_or(&""),
                preview_tail(
                    composer
                        .fields
                        .get(composer.editing_field())
                        .map(String::as_str)
                        .unwrap_or("")
                )
            );
        } else {
            let pages = composer_pages(composer);
            let page = composer.review_page.min(pages.len().saturating_sub(1));
            let title = if composer.reply_to.is_some() {
                "Review reply"
            } else {
                "Review instruction"
            };
            println!("{}, page {}/{}\r\n{}\r\n\r\nj/k pages; Enter sends or retries unchanged; Esc cancels.", title, page + 1, pages.len(), pages.get(page).map(String::as_str).unwrap_or(""));
            if let Some(error) = &composer.error {
                println!("\r\n{}\r\nEnter retries unchanged with the same identity; Esc discards the draft.", safe(error));
            }
        }
        return render(screen, &output);
    }
    if let Some(lookup) = &view.history_lookup {
        match lookup.kind {
            None => println!("History lookup: d decision or m message; Esc cancels."),
            Some(kind) => {
                let label = match kind {
                    HistoryKind::Decision => "decision",
                    HistoryKind::Message => "message",
                };
                println!(
                    "History lookup: {} ID: {}_\r\nEnter opens; Esc cancels.",
                    label,
                    preview_tail(&lookup.id)
                );
            }
        }
        if lookup.invalid {
            println!("Enter a positive numeric ID.");
        }
        return render(screen, &output);
    }
    if let Some(history) = &view.history {
        let (_, section) = history_section(history.section);
        let content = history
            .items
            .iter()
            .map(|item| safe(&item.to_string()))
            .collect::<Vec<_>>()
            .join("\n\n");
        screen.draw(|frame| {
            let area = frame.area();
            let regions = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(1),
                    Constraint::Min(1),
                    Constraint::Length(1),
                ])
                .split(area);
            let state = if history.loading {
                "loading"
            } else if history.error.is_some() {
                "unavailable"
            } else if history.next_offset.is_some() {
                "more"
            } else {
                "end"
            };
            frame.render_widget(
                Paragraph::new(format!("History | {section} | {state}")),
                regions[0],
            );
            if history.loading {
                frame.render_widget(Paragraph::new("Loading history..."), regions[1]);
            } else if let Some(error) = &history.error {
                frame.render_widget(
                    Paragraph::new(format!("History unavailable: {}. R retries.", safe(error)))
                        .wrap(Wrap { trim: false }),
                    regions[1],
                );
            } else if history.items.is_empty() {
                frame.render_widget(Paragraph::new("No records in this section."), regions[1]);
            } else {
                let width = usize::from(regions[1].width).max(1);
                let lines = wrap_visible(&content, width);
                let max = lines.len().saturating_sub(usize::from(regions[1].height));
                let (old_width, anchor, old_scroll) = history.anchor.get();
                let moved = history.scroll.get() != old_scroll;
                let scroll = if old_width != 0 && old_width != width && !moved {
                    lines
                        .iter()
                        .rposition(|(_, start)| *start <= anchor)
                        .unwrap_or(0)
                } else {
                    history.scroll.get()
                }
                .min(max);
                history.scroll.set(scroll);
                let next_anchor = if old_width == 0 || moved {
                    lines.get(scroll).map_or(0, |(_, start)| *start)
                } else {
                    anchor
                };
                history.anchor.set((width, next_anchor, scroll));
                history.max_scroll.set(max);
                let visible = lines
                    .iter()
                    .skip(scroll)
                    .take(usize::from(regions[1].height))
                    .map(|(line, _)| Line::from(line.as_str()))
                    .collect::<Vec<_>>();
                frame.render_widget(Paragraph::new(visible), regions[1]);
            }
            frame.render_widget(
                Paragraph::new("↑↓ scroll · PgUp/PgDn · 1-3 sections · q"),
                regions[2],
            );
        })?;
        return Ok(());
    }
    if let Some(overview) = &view.overview {
        println!("Overview\r\n");
        if view.connection_unavailable {
            println!("Unavailable: live connection disconnected.");
        } else if overview.loading || overview.dirty {
            println!("Loading current counts...");
        } else if let Some(error) = &overview.error {
            println!("Unavailable: {}\r\nR retries.", safe(error));
        } else if let Some(counts) = &overview.counts {
            println!("Pending decisions: {}", counts.decisions);
            println!("Retained operator messages: {}", counts.messages);
            println!("Open completion notices: {}", counts.notices);
            println!("\r\nMessages include previously read items.");
        } else {
            println!("Loading current counts...");
        }
        println!("\r\nR refreshes; q or Esc returns.");
        return render(screen, &output);
    }
    if let Some(search) = &view.search {
        screen.draw(|frame| {
            let area = frame.area();
            let regions = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(2),
                    Constraint::Min(1),
                    Constraint::Length(2),
                ])
                .split(area);
            let title = format!(
                "Search messages: {}{}",
                safe(&search.query),
                if search.editing { "_" } else { "" }
            );
            frame.render_widget(Paragraph::new(title).wrap(Wrap { trim: false }), regions[0]);
            if search.editing {
                frame.render_widget(
                    Paragraph::new("Enter searches text, sender, context or activity."),
                    regions[1],
                );
            } else if search.loading {
                frame.render_widget(Paragraph::new("Searching..."), regions[1]);
            } else if let Some(error) = &search.error {
                frame.render_widget(
                    Paragraph::new(format!("Search unavailable: {}. R retries.", safe(error)))
                        .wrap(Wrap { trim: false }),
                    regions[1],
                );
            } else if let Some(page) = &search.page {
                if page.messages.is_empty() {
                    frame.render_widget(
                        Paragraph::new("No matching messages on this page."),
                        regions[1],
                    );
                } else {
                    let height = usize::from(regions[1].height).max(1);
                    let start = search.selected.saturating_add(1).saturating_sub(height);
                    let rows = page
                        .messages
                        .iter()
                        .enumerate()
                        .skip(start)
                        .take(height)
                        .map(|(index, item)| {
                            let selected = index == search.selected;
                            let line = format!(
                                "{} {}: {} {}",
                                if selected { ">" } else { " " },
                                safe(&item.sender),
                                safe(&item.body),
                                safe(&item.context)
                            );
                            Line::from(Span::styled(
                                line,
                                if selected {
                                    Style::default()
                                        .fg(
                                            if std::env::var_os("NO_COLOR").is_none()
                                                && std::env::var("TERM").ok().as_deref()
                                                    != Some("dumb")
                                            {
                                                Color::Cyan
                                            } else {
                                                Color::Reset
                                            },
                                        )
                                        .add_modifier(Modifier::BOLD)
                                } else {
                                    Style::default()
                                },
                            ))
                        })
                        .collect::<Vec<_>>();
                    frame.render_widget(Paragraph::new(rows), regions[1]);
                }
            }
            frame.render_widget(
                Paragraph::new(if search.editing {
                    "Enter search · Esc cancel"
                } else {
                    "↑↓ select · Enter open · n/p pages\ne edit · Esc close"
                }),
                regions[2],
            );
        })?;
        return Ok(());
    }
    if view.inbox_view || view.notice_view || (view.detail_page.is_none() && !view.priority_edit) {
        return display_queue(screen, view);
    }
    if view.inbox_view {
        if let Some(status) = &view.status {
            println!("{}\r", safe(status));
        }
        if !view.inbox_loaded {
            println!("Loading operator inbox. i or q returns.");
        } else if let Some(item) = view.inbox.get(view.inbox_current) {
            let pages = inbox_pages(item);
            let page = view.inbox_page.min(pages.len().saturating_sub(1));
            let reply_hint = if item.message.expects_reply {
                "r reply; "
            } else {
                ""
            };
            let position = view
                .inbox_offset
                .saturating_add(view.inbox_current)
                .saturating_add(1);
            let total = if view.inbox_next.is_some() {
                "?".into()
            } else {
                view.inbox_offset
                    .saturating_add(view.inbox.len())
                    .to_string()
            };
            println!(
                "Operator inbox {}/{} | message {} | page {}/{}\r\n{}\r\n\r\n{}? help; q returns.",
                position,
                total,
                item.id,
                page + 1,
                pages.len(),
                pages.get(page).map(String::as_str).unwrap_or(""),
                reply_hint
            );
        } else {
            println!("Operator inbox: no messages. ? help; q returns.");
        }
        return render(screen, &output);
    }
    if view.notice_view {
        if let Some(status) = &view.status {
            println!("{}\r", safe(status));
        }
        if !view.notice_loaded {
            println!("Loading completion notices. n or q returns.");
        } else if let Some(notice) = view.notices.get(view.notice_current) {
            let pages = notice_pages(notice);
            let page = view.notice_page.min(pages.len().saturating_sub(1));
            let position = view
                .notice_offset
                .saturating_add(view.notice_current)
                .saturating_add(1);
            let total = if view.notice_next.is_some() {
                "?".into()
            } else {
                view.notice_offset
                    .saturating_add(view.notices.len())
                    .to_string()
            };
            println!("Completion notice {}/{} | notice {} | page {}/{}\r\n\r\n{}\r\n\r\nj/k pages; d dismiss; ? help; q returns.", position, total, notice.id, page + 1, pages.len(), pages.get(page).map(String::as_str).unwrap_or(""));
        } else {
            println!("No open completion notices. ? help; q returns.");
        }
        return render(screen, &output);
    }
    let notice_count = if !view.notice_loaded {
        "loading".into()
    } else if view.notice_next.is_some() {
        format!("{}+", view.notices.len())
    } else {
        view.notices.len().to_string()
    };
    let width = terminal::size().map(|(cols, _)| cols).unwrap_or(80);
    if width > 0 && width < 60 {
        println!("Open completion notices: {} (n)\r", notice_count);
    } else {
        println!(
            "Open completion notices: {} (n to view); i inbox; m compose\r",
            notice_count
        );
    }
    if let Some(status) = &view.status {
        println!("{}\r", safe(status));
    }
    if view.draft_request_missing {
        println!(
            "Request not on this queue page; draft retained. Check state before submitting.\r"
        );
    }
    if !view.request_loaded {
        println!("Loading requests. i inbox; ? help; q quits.\r");
        return render(screen, &output);
    }
    if let Some(pending) = view.current() {
        let request = &pending.request;
        if let Some(page) = view.detail_page {
            let pages = detail_pages(request, view.detail_option);
            let page = page.min(pages.len().saturating_sub(1));
            println!(
                "Decision details, page {}/{}\r\n\r\n{}\r\n\r\nj/k pages; c, q or Esc returns.",
                page + 1,
                pages.len(),
                pages.get(page).map(String::as_str).unwrap_or("")
            );
            return render(screen, &output);
        }
        println!(
            "{} queue: {}/{}{} | importance {}\r\n{}\r\n{}\r",
            "Decisions",
            view.request_offset + view.current + 1,
            view.request_offset + view.requests.len(),
            if view.request_next.is_some() { "+" } else { "" },
            pending.importance,
            preview(&format!("Request {} from {}", pending.id, request.agent)),
            preview(&request.question)
        );
        if let Some(explanation) = &request.explanation {
            println!("{}\r", preview(&format!("Explanation: {explanation}")));
        }
        let choices = view.choices();
        // Leave space for the queue header, explanation and help. Follow the
        // selection so a long list cannot scroll the selected choice away.
        let height = terminal::size().map(|(_, rows)| rows).unwrap_or(24);
        let height = if height == 0 { 24 } else { height };
        let capacity = usize::from(height).saturating_sub(10).max(1);
        let total = choices.len() + 1; // Custom answer is the last choice.
        let selected = view.selected.unwrap_or(0).min(total - 1);
        let start = selected.saturating_sub(capacity - 1);
        let end = start.saturating_add(capacity).min(total);
        for position in start..end {
            let selected_mark = if view.selected == Some(position) {
                ">"
            } else {
                " "
            };
            if let Some(index) = choices.get(position) {
                if let Some(option) = request.options.get(*index) {
                    let recommended = if Some(*index) == request.recommendation {
                        format!(
                            " (recommended: {})",
                            safe(request.reason.as_deref().unwrap_or(""))
                        )
                    } else {
                        String::new()
                    };
                    let line = format!(
                        "{} {}. {}{}",
                        selected_mark,
                        position + 1,
                        option,
                        recommended
                    );
                    let line = if Some(*index) == request.recommendation
                        && preview(&line) != safe(&line)
                    {
                        format!("{} {}. [rec] {}", selected_mark, position + 1, option)
                    } else {
                        line
                    };
                    println!("{}\r", preview(&line));
                }
            } else {
                println!("{} {}. Custom answer\r", selected_mark, position + 1);
            }
        }
        if total > capacity {
            println!("Choices {}-{} of {}\r", start + 1, end, total);
        }
        if view.priority_edit {
            println!("\r\nSet importance 0-5; Esc cancels.");
        } else if view.editing {
            println!(
                "\r\nCustom answer: {}_\r\nCtrl-R reviews; Enter submits; Esc cancels.",
                preview_tail(&view.custom)
            );
        } else {
            println!(
                "\r\nEnter confirms answer to {}; d {}; p importance; ? help.",
                pending.id, "defers"
            );
        }
    } else {
        println!("No {} requests. i inbox; ? help; q quits.", "pending");
    }
    render(screen, &output)
}

// Preserve every printable scalar while wrapping to the available terminal cells.
fn wrap_visible(text: &str, width: usize) -> Vec<(String, usize)> {
    let mut lines = Vec::new();
    let mut line = String::new();
    let mut used = 0;
    let mut start = 0;
    let mut offset = 0;
    for grapheme in text.graphemes(true) {
        if grapheme == "\n" {
            lines.push((std::mem::take(&mut line), start));
            used = 0;
            offset += 1;
            start = offset;
            continue;
        }
        let cells = UnicodeWidthStr::width(grapheme);
        if used + cells > width && !line.is_empty() {
            lines.push((std::mem::take(&mut line), start));
            used = 0;
            start = offset;
        }
        line.push_str(grapheme);
        used += cells;
        offset += grapheme.chars().count();
    }
    lines.push((line, start));
    lines
}

// Queue contents remain bounded to the current backend page. Page transitions
// still fetch asynchronously; the list never claims an exact total if more exist.
fn display_queue(screen: &mut Screen, view: &View) -> Result<()> {
    let (title, list_title, detail_title, rows, selected, more, loaded, detail) = if view.inbox_view
    {
        let rows = view
            .inbox
            .iter()
            .map(|item| format!("#{} {}", item.id, safe(&item.message.sender)))
            .collect::<Vec<_>>();
        let item = view.inbox.get(view.inbox_current);
        let detail = item
            .map(|item| {
                let reply = if view.suspended_instruction.is_none() && item.message.expects_reply
                    && matches!(&item.message.target, Target::Participant { name } if name == "operator") { "r reply · " } else { "" };
                format!(
                    "From: {}\nWork: {}\nMessage: {}\nActivity: {}\n\n{reply}H history",
                    safe(&item.message.sender),
                    safe(item.message.context.as_deref().unwrap_or("(none supplied)")),
                    safe(&item.message.body),
                    safe(
                        item.message
                            .activity
                            .as_deref()
                            .unwrap_or("(none supplied)")
                    )
                )
            })
            .unwrap_or_else(|| "No messages".into());
        (
            "Operator inbox",
            "Messages",
            item.map(|i| format!("Message #{}", i.id))
                .unwrap_or_else(|| "Message".into()),
            rows,
            view.inbox_current,
            view.inbox_next.is_some(),
            view.inbox_loaded,
            detail,
        )
    } else if view.notice_view {
        let rows = view
            .notices
            .iter()
            .map(|item| format!("#{} {}", item.id, safe(&item.notice.work)))
            .collect::<Vec<_>>();
        let item = view.notices.get(view.notice_current);
        let detail = item
            .map(|item| {
                format!(
                    "From: {}\nWork: {}\nResult: {}\nActivity: {}\n\nd dismiss notice #{}",
                    safe(&item.notice.agent),
                    safe(&item.notice.work),
                    safe(&item.notice.result),
                    safe(item.notice.activity.as_deref().unwrap_or("(none supplied)")),
                    item.id
                )
            })
            .unwrap_or_else(|| "No open completion notices".into());
        (
            "Completion notices",
            "Notices",
            item.map(|i| format!("Notice #{}", i.id))
                .unwrap_or_else(|| "Notice".into()),
            rows,
            view.notice_current,
            view.notice_next.is_some(),
            view.notice_loaded,
            detail,
        )
    } else {
        let rows = view
            .requests
            .iter()
            .map(|item| safe(&item.request.question))
            .collect::<Vec<_>>();
        let item = view.current();
        let detail = item
            .map(|item| {
                let r = &item.request;
                let mut text = format!(
                    "From: {}\nQuestion: {}\nImportance: {}\n",
                    safe(&r.agent),
                    safe(&r.question),
                    item.importance
                );
                if let Some(explanation) = &r.explanation {
                    text.push_str(&format!("Explanation: {}\n", safe(explanation)));
                }
                if let Some(context) = &r.context {
                    text.push_str(&format!("Context: {}\n", safe(context)));
                }
                text.push_str("Answers:\n");
                for (position, index) in view.choices().iter().enumerate() {
                    let option = r.options.get(*index).map(String::as_str).unwrap_or("");
                    let mark = if view.selected == Some(position) {
                        ">"
                    } else {
                        " "
                    };
                    let rec = if Some(*index) == r.recommendation {
                        format!(
                            " (recommended: {})",
                            safe(r.reason.as_deref().unwrap_or(""))
                        )
                    } else {
                        String::new()
                    };
                    let prefix = if Some(*index) == r.recommendation && option.chars().count() > 30
                    {
                        "[rec] "
                    } else {
                        ""
                    };
                    text.push_str(&format!(
                        "{mark} {}. {prefix}{}{}\n",
                        position + 1,
                        safe(option),
                        rec
                    ));
                }
                let mark = if view.selected == Some(view.choices().len()) {
                    ">"
                } else {
                    " "
                };
                text.push_str(&format!(
                    "{mark} {}. Custom answer{}",
                    view.choices().len() + 1,
                    if view.editing {
                        format!(": {}_", preview_tail(&view.custom))
                    } else {
                        String::new()
                    }
                ));
                text
            })
            .unwrap_or_else(|| "No decisions".into());
        (
            "Decisions",
            "Decisions",
            "Decision".into(),
            rows,
            view.current,
            view.request_next.is_some(),
            view.request_loaded,
            detail,
        )
    };
    let (rows, detail_title, detail) = if loaded {
        (rows, detail_title, detail)
    } else {
        (Vec::new(), list_title.to_string(), "Loading...".to_string())
    };
    let mut action_status = if view.busy {
        Some("Action in progress; outcome unconfirmed".to_string())
    } else if let Some(status) = &view.status {
        Some(
            if view.suspended_instruction.is_some() || view.suspended_answer.is_some() {
                format!("{status} · draft held")
            } else {
                status.clone()
            },
        )
    } else if view.suspended_instruction.is_some() {
        Some("Instruction draft suspended; m resumes".to_string())
    } else if view.suspended_answer.is_some() {
        Some("Answer draft suspended; Ctrl-O resumes".to_string())
    } else {
        None
    };
    if view.draft_request_missing {
        let warning =
            "Request not on this queue page; draft retained. Check state before submitting.";
        if let Some(status) = &mut action_status {
            status.push('\n');
            status.push_str(warning);
        } else {
            action_status = Some(warning.to_string());
        }
    }
    let refresh_status = if view.connection_unavailable {
        Some("Live updates disconnected; reconnecting")
    } else if terminal::size().is_ok_and(|(w, _)| w < 60) {
        view.poll_summary.as_deref()
    } else {
        view.poll_error.as_deref()
    };
    let use_color = std::env::var_os("NO_COLOR").is_none()
        && std::env::var("TERM").ok().as_deref() != Some("dumb");
    let accent = if use_color { Color::Cyan } else { Color::Reset };
    screen.draw(|frame| {
        let area = frame.area();
        if area.width < 40 || area.height < 12 {
            frame.render_widget(Paragraph::new("Resize terminal to at least 40 x 12"), area);
            return;
        }
        let footer_height = if refresh_status.is_some() && action_status.is_some() {
            3
        } else {
            2
        } + u16::from(
            view.draft_request_missing
                && (view.busy
                    || view.status.is_some()
                    || view.suspended_instruction.is_some()
                    || view.suspended_answer.is_some()),
        );
        let sections = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Min(1),
                Constraint::Length(footer_height),
            ])
            .split(area);
        let notices = if !view.notice_loaded {
            "loading".to_string()
        } else if view.notice_next.is_some() {
            format!("{}+", view.notices.len())
        } else {
            view.notices.len().to_string()
        };
        let arrival = "";
        let heading = if area.width < 60 {
            format!("Aviary {title} I:i N:{notices}{arrival}")
        } else {
            format!("Aviary  {title} | inbox | notices {notices}{arrival} | ? help")
        };
        frame.render_widget(
            Paragraph::new(heading).style(Style::default().fg(accent).add_modifier(Modifier::BOLD)),
            sections[0],
        );
        let horizontal = area.width >= 70;
        let panes = if horizontal {
            Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
                .split(sections[1])
        } else {
            Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(4), Constraint::Min(3)])
                .split(sections[1])
        };
        let focus_style = Style::default().fg(accent).add_modifier(Modifier::BOLD);
        let list_label = if view.list_focus {
            format!("[{list_title}]")
        } else {
            list_title.to_string()
        };
        let detail_label = if !view.list_focus {
            format!("[{detail_title}]")
        } else {
            detail_title.clone()
        };
        let height = usize::from(panes[0].height.saturating_sub(2));
        let capacity = height.saturating_sub(usize::from(!loaded || more)).max(1);
        let start = selected.saturating_sub(capacity.saturating_sub(1));
        let mut list = rows
            .iter()
            .enumerate()
            .skip(start)
            .take(capacity)
            .map(|(idx, row)| {
                let text = format!("{} {}", if idx == selected { ">" } else { " " }, row);
                if idx == selected {
                    Line::from(Span::styled(text, focus_style))
                } else {
                    Line::from(text)
                }
            })
            .collect::<Vec<_>>();
        if !loaded {
            list.push(Line::from("Loading..."));
        } else if more {
            list.push(Line::from("More available ↓"));
        }
        let list_block = Block::default()
            .borders(Borders::ALL)
            .title(list_label)
            .border_style(if view.list_focus {
                focus_style
            } else {
                Style::default()
            });
        frame.render_widget(Paragraph::new(list).block(list_block), panes[0]);
        let inner_width = usize::from(panes[1].width.saturating_sub(2)).max(1);
        let inner_height = usize::from(panes[1].height.saturating_sub(2)).max(1);
        let lines = wrap_visible(&detail, inner_width);
        let max_scroll = lines.len().saturating_sub(inner_height);
        let (saved_scroll, saved_anchor) = if view.inbox_view {
            (&view.inbox_scroll, &view.inbox_anchor)
        } else if view.notice_view {
            (&view.notice_scroll, &view.notice_anchor)
        } else {
            (&view.detail_scroll, &view.detail_anchor)
        };
        let (old_width, anchor, old_scroll) = saved_anchor.get();
        let moved = saved_scroll.get() != old_scroll;
        let mut scroll = if old_width != 0 && old_width != inner_width {
            lines
                .iter()
                .rposition(|(_, start)| *start <= anchor)
                .unwrap_or(0)
        } else {
            saved_scroll.get()
        }
        .min(max_scroll);
        if !view.inbox_view && !view.notice_view && view.follow_answer {
            if let Some(answer_line) = lines.iter().position(|(line, _)| line.starts_with("> ")) {
                scroll = answer_line
                    .saturating_sub(inner_height.saturating_sub(2))
                    .min(max_scroll);
            }
        }
        saved_scroll.set(scroll);
        let next_anchor = if old_width == 0
            || moved
            || (!view.inbox_view && !view.notice_view && view.follow_answer)
        {
            lines.get(scroll).map_or(0, |(_, start)| *start)
        } else {
            anchor
        };
        saved_anchor.set((inner_width, next_anchor, scroll));
        view.detail_max.set(max_scroll);
        view.detail_actual.set(scroll);
        let continuation = match (scroll > 0, scroll < max_scroll) {
            (true, true) => " ↑↓",
            (true, false) => " ↑",
            (false, true) => " ↓",
            _ => "",
        };
        let detail_block = Block::default()
            .borders(Borders::ALL)
            .title(format!("{detail_label}{continuation}"))
            .border_style(if !view.list_focus {
                focus_style
            } else {
                Style::default()
            });
        let content = lines
            .iter()
            .skip(scroll)
            .take(inner_height)
            .map(|(line, _)| {
                if line.starts_with("> ") {
                    Line::from(Span::styled(line.as_str(), focus_style))
                } else {
                    Line::from(line.as_str())
                }
            })
            .collect::<Vec<_>>();
        frame.render_widget(Paragraph::new(content).block(detail_block), panes[1]);
        let hint = if view.editing {
            "Enter submits · Ctrl-O suspends · Esc cancels"
        } else if view.inbox_view {
            "Tab focus · ↑↓ move · PgUp/PgDn · ?"
        } else if view.notice_view {
            "Tab focus · ↑↓ move · d dismiss · ?"
        } else {
            "Tab focus · ↑↓ · Enter confirms · ?"
        };
        let action_lines = action_status
            .as_deref()
            .map(|status| status.lines().map(safe).collect::<Vec<_>>().join("\n"));
        let footer = match (refresh_status, action_lines.as_deref()) {
            (Some(refresh), Some(action)) => {
                format!("{}\n{}\n{}", safe(refresh), action, hint)
            }
            (Some(refresh), None) => format!("{}\n{}", safe(refresh), hint),
            (None, Some(action)) => format!("{}\n{}", action, hint),
            (None, None) => format!("\n{hint}"),
        };
        frame.render_widget(Paragraph::new(footer), sections[2]);
    })?;
    Ok(())
}

fn render(screen: &mut Screen, output: &str) -> Result<()> {
    let lines: Vec<Line<'_>> = output
        .lines()
        .map(|line| {
            let line = line.trim_end_matches('\r');
            let use_color = std::env::var_os("NO_COLOR").is_none()
                && std::env::var("TERM").ok().as_deref() != Some("dumb");
            let style = if line.starts_with('>')
                || [
                    "Compose ",
                    "Review ",
                    "Keyboard help",
                    "History",
                    "Custom answer review",
                    "Request ",
                ]
                .iter()
                .any(|prefix| line.starts_with(prefix))
            {
                Style::default()
                    .fg(if use_color { Color::Cyan } else { Color::Reset })
                    .add_modifier(Modifier::BOLD)
            } else if line.contains("outcome unknown")
                || line.contains("outcome unconfirmed")
                || line.contains("unavailable")
                || line.starts_with("Unconfirmed:")
                || line.starts_with("Rejected:")
            {
                Style::default().fg(if use_color {
                    Color::Yellow
                } else {
                    Color::Reset
                })
            } else {
                Style::default()
            };
            Line::from(Span::styled(line, style))
        })
        .collect();
    screen.draw(|frame| {
        frame.render_widget(
            Paragraph::new(lines).wrap(Wrap { trim: false }),
            frame.area(),
        )
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::DecisionRequest;

    fn request(recommendation: Option<usize>, count: usize) -> PendingRequest {
        PendingRequest {
            id: 42,
            importance: 0,
            deferred: false,
            request: DecisionRequest {
                identity: "test".into(),
                agent: "agent".into(),
                question: "Choose".into(),
                explanation: None,
                context: None,
                activity: None,
                options: (0..count).map(|i| format!("option {i}")).collect(),
                importance: 0,
                recommendation,
                reason: recommendation.map(|_| "reason".into()),
            },
        }
    }
    fn key(c: KeyCode) -> KeyEvent {
        KeyEvent::new(c, KeyModifiers::NONE)
    }

    #[test]
    fn uncertain_deferral_retry_retains_target_without_a_current_item() {
        let mut view = View::new();
        view.defer_retry = Some((42, "original-identity".into()));
        match view.key(key(KeyCode::Char('U'))) {
            Action::Submit(Operation::DeferDecision { id, identity }) => {
                assert_eq!(id, 42);
                assert_eq!(identity, "original-identity");
            }
            _ => panic!("expected the original deferral to be retried"),
        }
    }

    #[test]
    fn help_preserves_selection_and_text_entry() {
        let mut view = View::new();
        view.update(vec![request(Some(1), 2)]);
        view.request_loaded = true;
        view.composer = Some(Composer::default());
        view.key(key(KeyCode::Char('x')));
        view.key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL));
        assert!(view.help);
        view.key(key(KeyCode::Char('z')));
        view.key(key(KeyCode::Esc));
        assert!(!view.help);
        assert_eq!(view.selected, Some(0));
        assert_eq!(view.current().map(|item| item.id), Some(42));
        assert_eq!(
            view.composer.as_ref().map(|c| c.fields[3].as_str()),
            Some("x")
        );
        view.key(key(KeyCode::Char('?')));
        assert_eq!(
            view.composer.as_ref().map(|c| c.fields[3].as_str()),
            Some("x?")
        );
    }

    #[test]
    fn instruction_requires_explicit_send_and_retains_identity_and_content() {
        let mut view = View::new();
        view.composer = Some(Composer::instruction().expect("random identity"));
        let first_id = view.composer.as_ref().expect("composer").fields[0].clone();
        assert_ne!(
            first_id,
            Composer::instruction().expect("random identity").fields[0]
        );
        for c in "Do work?".chars() {
            assert!(matches!(view.key(key(KeyCode::Char(c))), Action::Redraw));
        }
        for _ in 0..2 {
            assert!(
                matches!(view.key(key(KeyCode::Enter)), Action::SendInstruction(message)
                if message.identity == first_id && message.channel == "all"
                    && matches!(message.target, Target::Group) && message.body == "Do work?")
            );
        }
    }

    #[test]
    fn unconfirmed_instruction_locks_content_and_requires_discard_confirmation() {
        let mut view = View::new();
        view.composer = Some(Composer::instruction().expect("random identity"));
        for c in "original".chars() {
            view.key(key(KeyCode::Char(c)));
        }
        let composer = view.composer.as_mut().expect("composer");
        let identity = composer.fields[0].clone();
        composer.error = Some("Unconfirmed".into());
        view.key(key(KeyCode::Backspace));
        view.key(key(KeyCode::Char('x')));
        assert!(matches!(view.key(key(KeyCode::Enter)),
            Action::SendInstruction(message) if message.identity == identity && message.body == "original"));
        view.key(key(KeyCode::Esc));
        assert!(view.composer.as_ref().expect("composer").confirm_discard);
        view.key(key(KeyCode::Char('n')));
        assert!(view.composer.is_some());
        view.key(key(KeyCode::Esc));
        view.key(key(KeyCode::Char('y')));
        assert!(view.composer.is_none());
    }

    #[test]
    fn suspended_instruction_resumes_with_identity_and_content() {
        let mut view = View::new();
        view.composer = Some(Composer::instruction().expect("random identity"));
        for c in "keep this".chars() {
            view.key(key(KeyCode::Char(c)));
        }
        let identity = view.composer.as_ref().expect("composer").fields[0].clone();
        view.composer.as_mut().expect("composer").error = Some("Unconfirmed".into());
        view.key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL));
        assert!(view.composer.is_none());
        assert!(view.suspended_instruction.is_some());
        view.key(key(KeyCode::Char('m')));
        assert!(matches!(view.key(key(KeyCode::Enter)),
            Action::SendInstruction(message) if message.identity == identity && message.body == "keep this"));
    }

    #[test]
    fn wrapping_keeps_unicode_graphemes_intact_and_anchors_scalars() {
        let lines = wrap_visible("a👩‍💻e\u{301}鳥z", 3);
        assert_eq!(
            lines,
            vec![("a👩‍💻".into(), 0), ("e\u{301}鳥".into(), 4), ("z".into(), 7)]
        );
        assert_eq!(
            lines
                .iter()
                .map(|(line, _)| line.as_str())
                .collect::<String>(),
            "a👩‍💻e\u{301}鳥z"
        );
    }

    #[test]
    fn basic_color_writer_handles_split_sequences_and_preserves_other_colors() {
        let mut writer = BasicColorWriter {
            inner: Vec::new(),
            pending: Vec::new(),
        };
        writer
            .write_all(b"before\x1b[38;5;")
            .expect("buffer first part");
        writer
            .write_all(b"6;49mcyan\x1b[38;5;11myellow\x1b[38;5;60m")
            .expect("buffer second part");
        writer.flush().expect("flush terminal output");
        assert_eq!(
            writer.inner,
            b"before\x1b[36;49mcyan\x1b[33myellow\x1b[38;5;60m"
        );
    }

    #[test]
    fn list_focus_moves_decisions_without_confirming() {
        let mut view = View::new();
        let first = request(Some(0), 2);
        let mut second = request(Some(0), 2);
        second.id = 43;
        view.update(vec![first, second]);
        view.request_loaded = true;
        view.key(key(KeyCode::Tab));
        assert!(view.list_focus);
        assert!(matches!(view.key(key(KeyCode::Enter)), Action::None));
        assert!(matches!(view.key(key(KeyCode::Down)), Action::Redraw));
        assert_eq!(view.current().map(|item| item.id), Some(43));
        view.key(key(KeyCode::Tab));
        assert!(matches!(
            view.key(key(KeyCode::Enter)),
            Action::Submit(Operation::Answer { id: 43, .. })
        ));
    }

    #[test]
    fn recommendation_is_first_and_requires_enter() {
        let mut view = View::new();
        view.update(vec![request(Some(2), 3)]);
        view.request_loaded = true;
        assert_eq!(view.choices(), vec![2, 0, 1]);
        assert_eq!(view.selected, Some(0));
        assert!(matches!(
            view.key(key(KeyCode::Enter)),
            Action::Submit(Operation::Answer { id: 42, index: 2 })
        ));
        view.key(key(KeyCode::Char('2')));
        assert!(matches!(
            view.key(key(KeyCode::Enter)),
            Action::Submit(Operation::Answer { id: 42, index: 0 })
        ));
    }

    #[test]
    fn no_recommendation_and_many_choices_can_reach_custom() {
        let mut view = View::new();
        view.update(vec![request(None, 12)]);
        view.request_loaded = true;
        assert!(matches!(view.key(key(KeyCode::Enter)), Action::None));
        for _ in 0..13 {
            view.key(key(KeyCode::Char('j')));
        }
        assert_eq!(view.selected, Some(12));
        assert!(matches!(view.key(key(KeyCode::Enter)), Action::Redraw));
        for c in "鳥 q".chars() {
            view.key(key(KeyCode::Char(c)));
        }
        assert!(
            matches!(view.key(key(KeyCode::Enter)), Action::Submit(Operation::AnswerCustom { id: 42, value }) if value == "鳥 q")
        );
    }

    #[test]
    fn page_shift_keeps_custom_draft_with_original_request() {
        let mut view = View::new();
        view.update_request_page(RequestPage {
            requests: vec![request(None, 0)],
            next_offset: None,
            newest_id: Some(42),
        });
        view.key(key(KeyCode::Char('j')));
        view.key(key(KeyCode::Enter));
        for c in "draft".chars() {
            view.key(key(KeyCode::Char(c)));
        }
        let mut other = request(None, 0);
        other.id = 43;
        view.update_request_page(RequestPage {
            requests: vec![other.clone()],
            next_offset: Some(1),
            newest_id: Some(43),
        });
        assert!(view.draft_request_missing);
        assert_eq!(view.current().map(|item| item.id), Some(42));
        assert!(
            matches!(view.key(key(KeyCode::Enter)), Action::Submit(Operation::AnswerCustom { id: 42, value }) if value == "draft")
        );
        view.key(key(KeyCode::Esc));
        view.update_request_page(RequestPage {
            requests: vec![other],
            next_offset: Some(1),
            newest_id: Some(43),
        });
        assert_eq!(view.current().map(|item| item.id), Some(43));
        assert!(!view.draft_request_missing);
    }

    #[test]
    fn arrivals_show_transient_notification_without_clearing_draft() {
        let mut view = View::new();
        let page = |newest_id| RequestPage {
            requests: vec![request(None, 0)],
            next_offset: Some(1),
            newest_id,
        };
        view.update_request_page(page(Some(42)));
        view.key(key(KeyCode::Char('j')));
        view.key(key(KeyCode::Enter));
        view.key(key(KeyCode::Char('R')));
        view.update_request_page(page(Some(43)));
        assert_eq!(view.custom, "R");
        assert_eq!(view.status.as_deref(), Some("New decision received"));
        assert!(view.status_expires.is_some());
    }

    #[test]
    fn resizing_keeps_each_paged_view_at_its_prior_text_offset() {
        let mut view = View::new();
        view.page_size = 200;
        view.detail_page = Some(3);
        view.composer = Some(Composer {
            review_page: 3,
            ..Composer::default()
        });
        let history = HistoryView::new(HistoryKind::Decision, 42);
        history.scroll.set(3);
        view.history = Some(history);
        view.inbox_page = 3;
        view.notice_page = 3;
        view.resize_pages(114);
        assert_eq!(view.detail_page, Some(5));
        assert_eq!(view.composer.as_ref().map(|item| item.review_page), Some(5));
        assert_eq!(view.history.as_ref().map(|item| item.scroll.get()), Some(3));
        assert_eq!((view.inbox_page, view.notice_page), (5, 5));
    }

    #[test]
    fn controls_from_remote_content_are_not_emitted() {
        assert_eq!(safe("a\u{1b}[2J\nb"), "a [2J b");
    }
}
