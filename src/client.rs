use std::{
    io::{self, Write},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use crossterm::{
    cursor::Show,
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute, terminal,
};
use ratatui::{
    backend::CrosstermBackend,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
    Terminal,
};
use tokio::{sync::mpsc, time};

use crate::{
    error::Result,
    network,
    protocol::{
        HistoryKind, HistorySection, InboxItem, InboxPage, Message, MessageReply, NoticeItem,
        NoticePage, Operation, PendingRequest, RequestPage, Target,
    },
};

const POLL_INTERVAL: Duration = Duration::from_millis(500);
const POLL_TIMEOUT_MS: i32 = 1_000;

pub async fn wait_for_answer(address: &str, id: i64) -> Result<()> {
    loop {
        let response = network::call(address, &Operation::Get { id }).await?;
        if !response["answer"].is_null() {
            println!("{response}");
            return Ok(());
        }
        time::sleep(POLL_INTERVAL).await;
    }
}

pub async fn wait_for_message_response(address: &str, id: i64) -> Result<()> {
    loop {
        let response = network::call(address, &Operation::GetMessageResponse { id }).await?;
        if response
            .get("response")
            .is_some_and(|value| !value.is_null())
        {
            println!("{response}");
            return Ok(());
        }
        time::sleep(POLL_INTERVAL).await;
    }
}

async fn fetch_requests(address: &str, deferred: bool, offset: usize) -> Result<RequestPage> {
    let page: RequestPage = serde_json::from_value(
        network::call_with_timeout(
            address,
            &Operation::ListPage {
                deferred,
                offset,
                include_newest: true,
            },
            POLL_TIMEOUT_MS,
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
            POLL_TIMEOUT_MS,
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
            POLL_TIMEOUT_MS,
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
) -> Result<HistoryResult> {
    let (section, field) = history_section(section);
    let mut response = network::call_with_timeout(
        address,
        &Operation::HistoryPage {
            kind,
            id,
            section,
            offset,
        },
        POLL_TIMEOUT_MS,
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

struct HistoryView {
    kind: HistoryKind,
    id: i64,
    section: usize,
    offset: usize,
    previous: Vec<usize>,
    items: Vec<serde_json::Value>,
    next_offset: Option<usize>,
    current: usize,
    text_page: usize,
    loading: bool,
    needs_load: bool,
    error: Option<String>,
}
impl HistoryView {
    fn new(kind: HistoryKind, id: i64) -> Self {
        Self {
            kind,
            id,
            section: 0,
            offset: 0,
            previous: Vec::new(),
            items: Vec::new(),
            next_offset: None,
            current: 0,
            text_page: 0,
            loading: true,
            needs_load: true,
            error: None,
        }
    }
    fn reload(&mut self) {
        self.items.clear();
        self.next_offset = None;
        self.current = 0;
        self.text_page = 0;
        self.loading = true;
        self.needs_load = true;
        self.error = None;
    }
}

// Restore the terminal and stop the input thread on every ordinary exit path.
struct TerminalGuard {
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Drop for TerminalGuard {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        let _ = execute!(io::stdout(), Show);
        let _ = terminal::disable_raw_mode();
        print!("\r\n");
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
}
impl Composer {
    fn review_stage(&self) -> usize {
        if self.reply_to.is_some() {
            2
        } else {
            4
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
    page_size: usize,
    requests: Vec<PendingRequest>,
    request_offset: usize,
    request_previous: Vec<usize>,
    request_next: Option<usize>,
    request_loaded: bool,
    request_select_last: Option<bool>,
    arrivals: Arrivals,
    composer: Option<Composer>,
    status: Option<String>,
    poll_error: Option<String>,
    poll_summary: Option<String>,
    busy: bool,
    history: Option<HistoryView>,
    history_lookup: Option<HistoryLookup>,
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
    deferred_view: bool,
    priority_edit: bool,
    detail_page: Option<usize>,
    detail_option: Option<usize>,
    selected: Option<usize>,
    editing: bool,
    custom_review: Option<usize>,
    custom: String,
    draft_request_missing: bool,
}
impl View {
    fn new() -> Self {
        Self {
            page_size: detail_page_size(),
            requests: Vec::new(),
            request_offset: 0,
            request_previous: Vec::new(),
            request_next: None,
            request_loaded: false,
            request_select_last: None,
            arrivals: Arrivals::default(),
            composer: None,
            status: None,
            poll_error: None,
            poll_summary: None,
            busy: false,
            history: None,
            history_lookup: None,
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
            deferred_view: false,
            priority_edit: false,
            detail_page: None,
            detail_option: None,
            selected: None,
            editing: false,
            custom_review: None,
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
        if let Some(page) = &mut self.custom_review {
            remap(page);
        }
        if let Some(composer) = &mut self.composer {
            remap(&mut composer.review_page);
        }
        if let Some(history) = &mut self.history {
            remap(&mut history.text_page);
        }
        remap(&mut self.inbox_page);
        remap(&mut self.notice_page);
        self.page_size = new_size;
    }
    fn reset_selection(&mut self) {
        self.selected = self
            .current()
            .and_then(|r| r.request.recommendation.map(|_| 0));
        self.editing = false;
        self.custom_review = None;
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
        if let Some(composer) = &mut self.composer {
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
                    let valid = (composer.reply_to.is_none() && composer.stage == 2)
                        || composer
                            .fields
                            .get(composer.stage)
                            .is_some_and(|field| !field.is_empty());
                    if valid {
                        composer.stage += 1;
                        composer.error = None;
                    }
                }
                KeyCode::Backspace if composer.stage < composer.review_stage() => {
                    if let Some(field) = composer.fields.get_mut(composer.stage) {
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
                    if let Some(field) = composer.fields.get_mut(composer.stage) {
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
                    history.reload();
                    return Action::LoadHistory;
                }
                KeyCode::Char(']') => {
                    if let Some(next) = history.next_offset {
                        history.previous.push(history.offset);
                        history.offset = next;
                        history.reload();
                        return Action::LoadHistory;
                    }
                }
                KeyCode::Char('[') => {
                    if let Some(previous) = history.previous.pop() {
                        history.offset = previous;
                        history.reload();
                        return Action::LoadHistory;
                    }
                }
                KeyCode::Char('h') | KeyCode::Left => {
                    history.current = history.current.saturating_sub(1);
                    history.text_page = 0;
                }
                KeyCode::Char('l') | KeyCode::Right => {
                    history.current =
                        (history.current + 1).min(history.items.len().saturating_sub(1));
                    history.text_page = 0;
                }
                KeyCode::Char('j') | KeyCode::Down => {
                    let last = history
                        .items
                        .get(history.current)
                        .map(|item| text_pages(&item.to_string()).len().saturating_sub(1))
                        .unwrap_or(0);
                    history.text_page = (history.text_page + 1).min(last);
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    history.text_page = history.text_page.saturating_sub(1)
                }
                _ => return Action::None,
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
                KeyCode::Char('r') if self.inbox_loaded => {
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
                    let last = self
                        .inbox
                        .get(self.inbox_current)
                        .map(|item| inbox_pages(item).len().saturating_sub(1))
                        .unwrap_or(0);
                    self.inbox_page = (self.inbox_page + 1).min(last);
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.inbox_page = self.inbox_page.saturating_sub(1)
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
                    let last = self
                        .notices
                        .get(self.notice_current)
                        .map(|item| notice_pages(item).len().saturating_sub(1))
                        .unwrap_or(0);
                    self.notice_page = (self.notice_page + 1).min(last);
                }
                KeyCode::Char('k') | KeyCode::Up if self.notice_loaded => {
                    self.notice_page = self.notice_page.saturating_sub(1)
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
        if let Some(page) = self.custom_review {
            match key.code {
                KeyCode::Esc | KeyCode::Char('q') => self.custom_review = None,
                KeyCode::Char('j') | KeyCode::Down | KeyCode::Right => {
                    let last = text_pages(&self.custom).len().saturating_sub(1);
                    self.custom_review = Some((page + 1).min(last));
                }
                KeyCode::Char('k') | KeyCode::Up | KeyCode::Left => {
                    self.custom_review = Some(page.saturating_sub(1));
                }
                _ => return Action::None,
            }
            return Action::Redraw;
        }
        if self.editing {
            match key.code {
                KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.custom_review = Some(0);
                }
                KeyCode::Esc => self.editing = false,
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
        if key.code == KeyCode::Char('g') {
            self.history_lookup = Some(HistoryLookup::default());
            return Action::Redraw;
        }
        if key.code == KeyCode::Char('i') {
            return Action::ToggleInbox;
        }
        if key.code == KeyCode::Char('m') {
            self.composer = Some(Composer::default());
            self.status = None;
            return Action::Redraw;
        }
        if key.code == KeyCode::Char('n') {
            return Action::ToggleNotices;
        }
        if key.code == KeyCode::Tab {
            return Action::ToggleDeferred;
        }
        if key.code == KeyCode::Char('R') {
            self.arrivals.pending = false;
            return Action::Redraw;
        }
        if !self.request_loaded {
            return if key.code == KeyCode::Char('q') {
                Action::Quit
            } else {
                Action::None
            };
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
            KeyCode::Char('d') => Action::Submit(Operation::SetDeferred {
                id: request.id,
                deferred: !self.deferred_view,
            }),
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
                self.selected = Some(self.selected.map_or(0, |i| (i + 1).min(choices.len())));
                Action::Redraw
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.selected = Some(self.selected.map_or(0, |i| i.saturating_sub(1)));
                Action::Redraw
            }
            KeyCode::Char(c @ '1'..='9') => {
                let position = c as usize - '1' as usize;
                if position <= choices.len() {
                    self.selected = Some(position);
                    Action::Redraw
                } else {
                    Action::None
                }
            }
            KeyCode::Enter => match self.selected {
                Some(position) if position == choices.len() => {
                    self.editing = true;
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
    ToggleDeferred,
    ReloadRequests,
    ToggleInbox,
    ReloadInbox,
    ToggleNotices,
    ReloadNotices,
    OpenHistory(HistoryKind, i64),
    CloseHistory,
    LoadHistory,
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
    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = stop.clone();
    let (sender, mut receiver) = mpsc::unbounded_channel();
    let thread = std::thread::spawn(move || {
        while !thread_stop.load(Ordering::Relaxed) {
            match event::poll(Duration::from_millis(100)) {
                Ok(true) => match event::read() {
                    Ok(event @ (Event::Key(_) | Event::Resize(_, _))) => {
                        if sender.send(Ok(event)).is_err() {
                            break;
                        }
                    }
                    Ok(_) => {}
                    Err(error) => {
                        let _ = sender.send(Err(error));
                        break;
                    }
                },
                Ok(false) => {}
                Err(error) => {
                    let _ = sender.send(Err(error));
                    break;
                }
            }
        }
    });
    let _guard = TerminalGuard {
        stop,
        thread: Some(thread),
    };
    let mut interval = time::interval(POLL_INTERVAL);
    let mut view = View::new();
    let mut screen = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    let mut generation = 0u64;
    let mut refresh: Option<tokio::task::JoinHandle<_>> = None;
    let mut submission: Option<tokio::task::JoinHandle<(bool, bool, Result<serde_json::Value>)>> =
        None;
    let mut history_task: Option<tokio::task::JoinHandle<_>> = None;
    display(&mut screen, &view)?;
    loop {
        tokio::select! {
            _ = interrupt.recv() => return Ok(()),
            _ = terminate.recv() => return Ok(()),
            _ = interval.tick(), if (view.history.as_ref().is_some_and(|history| history.needs_load) && history_task.is_none()) || (view.history.is_none() && view.composer.is_none() && refresh.is_none() && submission.is_none()) => {
                if let Some(history) = &mut view.history {
                    history.needs_load = false;
                    let (kind, id, section, offset) = (history.kind, history.id, history.section, history.offset);
                    let address = address.to_owned();
                    let started = generation;
                    history_task = Some(tokio::spawn(async move { (started, fetch_history(&address, kind, id, section, offset).await) }));
                } else {
                    let address = address.to_owned();
                    let deferred = view.deferred_view;
                    let request_offset = view.request_offset;
                    let inbox_view = view.inbox_view;
                    let inbox_offset = view.inbox_offset;
                    let notice_offset = view.notice_offset;
                    let current_generation = generation;
                    refresh = Some(tokio::spawn(async move {
                        let result = tokio::join!(
                            fetch_requests(&address, deferred, request_offset),
                            fetch_notices(&address, notice_offset),
                            async { if inbox_view { Some(fetch_operator_inbox(&address, inbox_offset).await) } else { None } },
                        );
                        (current_generation, deferred, inbox_view, result)
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
                let (started, deferred, inbox_view, refreshed) = result.map_err(|error| crate::error::AppError::Task(error.to_string()))?;
                // A refresh started before a queue switch or action must never
                // replace newer UI state with an obsolete snapshot.
                if started != generation || deferred != view.deferred_view || inbox_view != view.inbox_view || view.composer.is_some() || view.history.is_some() || view.busy {
                    continue;
                }
                let (requests, notices, messages) = refreshed;
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
                let error = if errors.is_empty() { None } else { Some(format!("Refresh incomplete: {}; retrying", errors.iter().map(|(name, error)| format!("{name}: {error}")).collect::<Vec<_>>().join("; "))) };
                if view.poll_summary != summary { view.poll_summary = summary; changed = true; }
                if view.poll_error != error { view.poll_error = error; changed = true; }
                if changed { display(&mut screen, &view)?; }
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
                            Ok(page) => { history.items = page.items; history.next_offset = page.next_offset; history.error = None; }
                            Err(error) => { history.error = Some(error.to_string()); }
                        }
                        display(&mut screen, &view)?;
                    }
                }
                if view.history.as_ref().is_some_and(|history| history.needs_load) { interval.reset_immediately(); }
            }
            result = async {
                match submission.as_mut() {
                    Some(task) => task.await,
                    None => std::future::pending().await,
                }
            }, if submission.is_some() => {
                submission = None;
                view.busy = false;
                let (is_instruction, is_notice, result) = result.map_err(|error| crate::error::AppError::Task(error.to_string()))?;
                if is_instruction {
                    match result {
                        Ok(response) => {
                            let label = if view.composer.as_ref().is_some_and(|composer| composer.reply_to.is_some()) { "Reply" } else { "Instruction" };
                            view.composer = None;
                            view.status = Some(format!("{label} accepted: {response}"));
                        }
                        Err(error) => { if let Some(composer) = &mut view.composer { composer.error = Some(error.to_string()); } }
                    }
                } else {
                    match result {
                        Ok(_) => { view.status = None; if !is_notice { view.reset_selection(); view.reset_request_page(); } view.reset_notice_page(); }
                        Err(error) => { view.status = Some(format!("Not confirmed: {error}; check state before retry")); }
                    }
                }
                display(&mut screen, &view)?;
                interval.reset_immediately();
            }
            key = receiver.recv() => {
                let Some(event) = key else { return Ok(()); };
                let key = match event? {
                    Event::Key(key) => key,
                    Event::Resize(width, height) => { view.resize_pages(page_size_for(width, height)); display(&mut screen, &view)?; continue; },
                    _ => continue,
                };
                if view.busy {
                    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                        return Ok(());
                    }
                    continue;
                }
                match view.key(key) {
                    Action::None => {},
                    Action::Redraw => display(&mut screen, &view)?,
                    Action::Quit => return Ok(()),
                    Action::OpenHistory(kind, id) => {
                        generation = generation.wrapping_add(1);
                        view.history_lookup = None;
                        view.history = Some(HistoryView::new(kind, id));
                        display(&mut screen, &view)?; interval.reset_immediately();
                    },
                    Action::CloseHistory => {
                        generation = generation.wrapping_add(1);
                        view.history = None;
                        display(&mut screen, &view)?; interval.reset_immediately();
                    },
                    Action::LoadHistory => {
                        generation = generation.wrapping_add(1);
                        display(&mut screen, &view)?; interval.reset_immediately();
                    },
                    Action::ToggleNotices => {
                        generation = generation.wrapping_add(1);
                        view.notice_view = !view.notice_view;
                        view.reset_notice_page();
                        display(&mut screen, &view)?; interval.reset_immediately();
                    },
                    Action::ReloadNotices => {
                        generation = generation.wrapping_add(1);
                        display(&mut screen, &view)?; interval.reset_immediately();
                    },
                    Action::ToggleInbox => {
                        generation = generation.wrapping_add(1);
                        view.inbox_view = !view.inbox_view;
                        view.inbox_loaded = false;
                        view.inbox_page = 0;
                        display(&mut screen, &view)?;
                        interval.reset_immediately();
                    },
                    Action::ReloadInbox => {
                        generation = generation.wrapping_add(1);
                        display(&mut screen, &view)?;
                        interval.reset_immediately();
                    },
                    Action::ToggleDeferred => {
                        generation = generation.wrapping_add(1);
                        view.deferred_view = !view.deferred_view;
                        view.reset_request_page();
                        display(&mut screen, &view)?;
                        interval.reset_immediately();
                    },
                    Action::ReloadRequests => {
                        generation = generation.wrapping_add(1);
                        display(&mut screen, &view)?;
                        interval.reset_immediately();
                    },
                    Action::SendInstruction(message) => {
                        generation = generation.wrapping_add(1);
                        if let Some(composer) = &mut view.composer { composer.error = None; }
                        view.busy = true;
                        display(&mut screen, &view)?;
                        let address = address.to_owned();
                        submission = Some(tokio::spawn(async move {
                            (true, false, network::call(&address, &Operation::SendMessage { message }).await)
                        }));
                    },
                    Action::SendReply(reply) => {
                        generation = generation.wrapping_add(1);
                        if let Some(composer) = &mut view.composer { composer.error = None; }
                        view.busy = true;
                        display(&mut screen, &view)?;
                        let address = address.to_owned();
                        submission = Some(tokio::spawn(async move {
                            (true, false, network::call(&address, &Operation::ReplyMessage { reply }).await)
                        }));
                    },
                    Action::Submit(operation) => {
                        generation = generation.wrapping_add(1);
                        view.status = None;
                        view.busy = true;
                        display(&mut screen, &view)?;
                        let address = address.to_owned();
                        let is_notice = matches!(&operation, Operation::DismissNotice { .. });
                        submission = Some(tokio::spawn(async move {
                            (false, is_notice, network::call(&address, &operation).await)
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
            "Answer: {}   Recommendation reason: {}",
            safe(answer),
            safe(reason)
        )
    } else {
        format!(
            "Agent: {}   Question: {}   Explanation: {}   Supporting context: {}",
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
fn display(screen: &mut Terminal<CrosstermBackend<io::Stdout>>, view: &View) -> Result<()> {
    use std::fmt::Write as _;
    let mut output = String::new();
    macro_rules! println {
        ($($arg:tt)*) => {{ let _ = writeln!(&mut output, $($arg)*); }};
    }
    if view.busy {
        println!("Sending; outcome unknown until confirmed. Ctrl-C quits.\r");
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
    let new_arrival = view.arrivals.pending;
    if new_arrival
        && (view.inbox_view || view.notice_view)
        && view.composer.is_none()
        && view.history.is_none()
    {
        println!("New decisions (R in queue)\r");
    }
    if let Some(composer) = &view.composer {
        let labels = [
            "Retry identity (unique deployment-wide)",
            "Channel (must include operator)",
            "Recipient label (empty means group)",
            "Instruction text",
        ];
        if composer.stage < composer.review_stage() {
            let reply_labels = ["Retry identity (unique deployment-wide)", "Reply text"];
            let (title, labels) = if composer.reply_to.is_some() {
                ("Compose reply", reply_labels.as_slice())
            } else {
                ("Compose instruction", labels.as_slice())
            };
            println!(
                "{}: {}\r\n{}\r\n\r\nEnter continues; Esc cancels.",
                title,
                labels.get(composer.stage).unwrap_or(&""),
                preview_tail(
                    composer
                        .fields
                        .get(composer.stage)
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
                println!("\r\nSend failed or outcome unknown: {}\r\nRetry with the same identity, or retain it before cancelling.", safe(error));
            }
        }
        return render(screen, &output);
    }
    if let Some(page) = view.custom_review {
        let pages = text_pages(&view.custom);
        let page = page.min(pages.len().saturating_sub(1));
        println!(
            "Custom answer review, page {}/{}\r\n{}\r\n\r\nj/k pages; q or Esc returns to editing.",
            page + 1,
            pages.len(),
            pages.get(page).map(String::as_str).unwrap_or("")
        );
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
        let position = if history.loading {
            "loading"
        } else if history.error.is_some() {
            "error"
        } else if history.next_offset.is_some() {
            "more"
        } else {
            "end"
        };
        println!(
            "History of {} | {} | offset {} | {}\r",
            history.id, section, history.offset, position
        );
        if history.loading {
            println!("Loading history...");
        } else if let Some(error) = &history.error {
            println!("History unavailable: {}. R retries.", safe(error));
        } else if let Some(item) = history.items.get(history.current) {
            let pages = text_pages(&item.to_string());
            let page = history.text_page.min(pages.len().saturating_sub(1));
            println!(
                "Record {}/{} | text {}/{}\r\n{}",
                history.current + 1,
                history.items.len(),
                page + 1,
                pages.len(),
                pages.get(page).map(String::as_str).unwrap_or("")
            );
        } else {
            println!("No records in this section.");
        }
        println!("\r\n1/2/3 sections; h/l records; j/k text; [/] page; R refresh; q returns.");
        return render(screen, &output);
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
            println!("Operator inbox {}/{} | page {}/{}\r\n{}\r\n\r\nh/l messages; j/k pages; H history; g lookup; {}i or q returns.", position, total, page + 1, pages.len(), pages.get(page).map(String::as_str).unwrap_or(""), reply_hint);
        } else {
            println!("Operator inbox: no messages. i or q returns.");
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
            println!("Completion notice {}/{} | page {}/{}\r\n\r\n{}\r\n\r\nh/l notices; j/k pages; d dismiss; n or q returns.", position, total, page + 1, pages.len(), pages.get(page).map(String::as_str).unwrap_or(""));
        } else {
            println!("No open completion notices. n or q returns.");
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
    if new_arrival {
        println!("Open completion notices: {} | NEW (R)\r", notice_count);
    } else if width > 0 && width < 60 {
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
        println!("Loading requests. i inbox; g history; Tab queues; q quits.\r");
        return render(screen, &output);
    }
    if let Some(pending) = view.current() {
        let request = &pending.request;
        if let Some(page) = view.detail_page {
            let pages = detail_pages(request, view.detail_option);
            let page = page.min(pages.len().saturating_sub(1));
            println!(
                "Request {} details, page {}/{}\r\n\r\n{}\r\n\r\nj/k pages; c, q or Esc returns.",
                pending.id,
                page + 1,
                pages.len(),
                pages.get(page).map(String::as_str).unwrap_or("")
            );
            return render(screen, &output);
        }
        println!(
            "{} queue: {}/{}{} | importance {}\r\n{}\r\n{}\r",
            if view.deferred_view {
                "Deferred"
            } else {
                "Active"
            },
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
            let width = terminal::size().map(|(cols, _)| cols).unwrap_or(80);
            if width > 0 && width < 60 {
                println!(
                    "j/k select; Enter confirms; c/o detail\r\nh/l requests; d defer; Tab; q quit"
                );
            } else {
                println!("\r\nj/k select; h/l requests; 1-9 select; Enter confirms; d defer/restore; p priority; c context; o answer detail; H history; g lookup; Tab queues; q quits.");
            }
        }
    } else {
        println!(
            "No {} requests. i inbox; g history; Tab queues; q quits.",
            if view.deferred_view {
                "deferred"
            } else {
                "active"
            }
        );
    }
    render(screen, &output)
}

fn render(screen: &mut Terminal<CrosstermBackend<io::Stdout>>, output: &str) -> Result<()> {
    let lines: Vec<Line<'_>> = output
        .lines()
        .map(|line| {
            let line = line.trim_end_matches('\r');
            let style = if line.starts_with('>') {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else if line.contains("outcome unknown") || line.contains("unavailable") {
                Style::default().fg(Color::Yellow)
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
    fn arrival_indicator_tracks_new_ids_outside_the_current_page() {
        let mut view = View::new();
        let page = |newest_id| RequestPage {
            requests: vec![request(None, 0)],
            next_offset: Some(1),
            newest_id,
        };
        view.update_request_page(page(Some(42)));
        assert!(!view.arrivals.pending);
        view.update_request_page(page(Some(43)));
        assert!(view.arrivals.pending);
        assert_eq!(view.current().map(|item| item.id), Some(42));
        view.key(key(KeyCode::Char('j')));
        view.key(key(KeyCode::Enter));
        view.key(key(KeyCode::Char('R')));
        assert_eq!(view.custom, "R");
        assert!(view.arrivals.pending);
        view.key(key(KeyCode::Esc));
        view.key(key(KeyCode::Char('R')));
        assert!(!view.arrivals.pending);
        view.update_request_page(page(Some(43)));
        assert!(!view.arrivals.pending);
        view.update_request_page(page(Some(44)));
        assert!(view.arrivals.pending);
    }

    #[test]
    fn resizing_keeps_each_paged_view_at_its_prior_text_offset() {
        let mut view = View::new();
        view.page_size = 200;
        view.detail_page = Some(3);
        view.custom_review = Some(3);
        view.composer = Some(Composer {
            review_page: 3,
            ..Composer::default()
        });
        let mut history = HistoryView::new(HistoryKind::Decision, 42);
        history.text_page = 3;
        view.history = Some(history);
        view.inbox_page = 3;
        view.notice_page = 3;
        view.resize_pages(114);
        assert_eq!(view.detail_page, Some(5));
        assert_eq!(view.custom_review, Some(5));
        assert_eq!(view.composer.as_ref().map(|item| item.review_page), Some(5));
        assert_eq!(view.history.as_ref().map(|item| item.text_page), Some(5));
        assert_eq!((view.inbox_page, view.notice_page), (5, 5));
    }

    #[test]
    fn controls_from_remote_content_are_not_emitted() {
        assert_eq!(safe("a\u{1b}[2J\nb"), "a [2J b");
    }
}
