use std::{
    io::{self, Write},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    terminal,
};
use tokio::{sync::mpsc, time};

use crate::{
    error::Result,
    network,
    protocol::{Message, NoticeItem, NoticePage, Operation, PendingRequest, RequestPage, Target},
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

async fn fetch_requests(address: &str, deferred: bool) -> Result<Vec<PendingRequest>> {
    let mut result = Vec::new();
    let mut offset = 0;
    loop {
        let page: RequestPage = serde_json::from_value(
            network::call_with_timeout(
                address,
                &Operation::ListPage { deferred, offset },
                POLL_TIMEOUT_MS,
            )
            .await?,
        )?;
        for request in &page.requests {
            request.request.validate()?;
        }
        result.extend(page.requests);
        match page.next_offset {
            Some(next) if next > offset => offset = next,
            Some(_) => return Err(crate::error::AppError::InvalidFrame),
            None => return Ok(result),
        }
    }
}

async fn fetch_notices(address: &str) -> Result<Vec<NoticeItem>> {
    let mut result = Vec::new();
    let mut offset = 0;
    loop {
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
        result.extend(page.notices);
        match page.next_offset {
            Some(next) if next > offset => offset = next,
            Some(_) => return Err(crate::error::AppError::InvalidFrame),
            None => return Ok(result),
        }
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
        let _ = terminal::disable_raw_mode();
        print!("\r\n");
        let _ = io::stdout().flush();
    }
}

#[derive(Default)]
struct Composer {
    stage: usize,
    review_page: usize,
    fields: [String; 4],
    error: Option<String>,
}
impl Composer {
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

struct View {
    requests: Vec<PendingRequest>,
    composer: Option<Composer>,
    status: Option<String>,
    poll_error: Option<String>,
    busy: bool,
    notices: Vec<NoticeItem>,
    notice_view: bool,
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
}
impl View {
    fn new() -> Self {
        Self {
            requests: Vec::new(),
            composer: None,
            status: None,
            poll_error: None,
            busy: false,
            notices: Vec::new(),
            notice_view: false,
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
        }
    }
    fn current(&self) -> Option<&PendingRequest> {
        self.requests.get(self.current)
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
    }
    fn update(&mut self, requests: Vec<PendingRequest>) -> bool {
        if self.requests == requests {
            return false;
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
    fn update_notices(&mut self, notices: Vec<NoticeItem>) -> bool {
        if self.notices == notices {
            return false;
        }
        let previous = self.notices.get(self.notice_current).map(|n| n.id);
        self.notice_current = previous
            .and_then(|id| notices.iter().position(|n| n.id == id))
            .unwrap_or(0);
        if previous != notices.get(self.notice_current).map(|n| n.id) {
            self.notice_page = 0;
        }
        self.notices = notices;
        true
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
                KeyCode::Char('j') | KeyCode::Down | KeyCode::Right if composer.stage == 4 => {
                    let last = composer_pages(composer).len().saturating_sub(1);
                    composer.review_page = (composer.review_page + 1).min(last);
                }
                KeyCode::Char('k') | KeyCode::Up | KeyCode::Left if composer.stage == 4 => {
                    composer.review_page = composer.review_page.saturating_sub(1);
                }
                KeyCode::Enter if composer.stage == 4 => {
                    if let Some(message) = composer.message() {
                        return Action::SendInstruction(message);
                    }
                }
                KeyCode::Enter => {
                    let valid = composer.stage == 2
                        || composer
                            .fields
                            .get(composer.stage)
                            .is_some_and(|field| !field.is_empty());
                    if valid {
                        composer.stage += 1;
                        composer.error = None;
                    }
                }
                KeyCode::Backspace if composer.stage < 4 => {
                    if let Some(field) = composer.fields.get_mut(composer.stage) {
                        field.pop();
                    }
                }
                KeyCode::Char(c)
                    if composer.stage < 4
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
        if self.notice_view {
            match key.code {
                KeyCode::Char('n') | KeyCode::Char('q') | KeyCode::Esc => {
                    self.notice_view = false;
                    return Action::Redraw;
                }
                KeyCode::Char('h') | KeyCode::Left => {
                    self.notice_current = self.notice_current.saturating_sub(1);
                    self.notice_page = 0;
                    return Action::Redraw;
                }
                KeyCode::Char('l') | KeyCode::Right => {
                    self.notice_current =
                        (self.notice_current + 1).min(self.notices.len().saturating_sub(1));
                    self.notice_page = 0;
                    return Action::Redraw;
                }
                KeyCode::Char('j') | KeyCode::Down => {
                    let max = self
                        .notices
                        .get(self.notice_current)
                        .map(|n| notice_pages(n).len().saturating_sub(1))
                        .unwrap_or(0);
                    self.notice_page = (self.notice_page + 1).min(max);
                    return Action::Redraw;
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.notice_page = self.notice_page.saturating_sub(1);
                    return Action::Redraw;
                }
                KeyCode::Char('d') => {
                    if let Some(notice) = self.notices.get(self.notice_current) {
                        return Action::Submit(Operation::DismissNotice { id: notice.id });
                    }
                }
                _ => {}
            }
            return Action::None;
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
        if key.code == KeyCode::Char('m') {
            self.composer = Some(Composer::default());
            self.status = None;
            return Action::Redraw;
        }
        if key.code == KeyCode::Char('n') {
            self.notice_view = true;
            self.notice_page = 0;
            return Action::Redraw;
        }
        if key.code == KeyCode::Tab {
            return Action::ToggleDeferred;
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
                self.current = self.current.saturating_sub(1);
                self.reset_selection();
                Action::Redraw
            }
            KeyCode::Char('l') | KeyCode::Right => {
                self.current = (self.current + 1).min(self.requests.len().saturating_sub(1));
                self.reset_selection();
                Action::Redraw
            }
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
    SendInstruction(Message),
    Submit(Operation),
}

pub async fn terminal(address: &str) -> Result<()> {
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
    let mut generation = 0u64;
    let mut refresh: Option<tokio::task::JoinHandle<_>> = None;
    let mut submission: Option<tokio::task::JoinHandle<(bool, Result<serde_json::Value>)>> = None;
    display(&view)?;
    loop {
        tokio::select! {
            _ = interval.tick(), if view.composer.is_none() && refresh.is_none() && submission.is_none() => {
                let address = address.to_owned();
                let deferred = view.deferred_view;
                let current_generation = generation;
                refresh = Some(tokio::spawn(async move {
                    let result = async {
                        let requests = fetch_requests(&address, deferred).await?;
                        let notices = fetch_notices(&address).await?;
                        Ok::<_, crate::error::AppError>((requests, notices))
                    }.await;
                    (current_generation, deferred, result)
                }));
            }
            result = async {
                match refresh.as_mut() {
                    Some(task) => task.await,
                    None => std::future::pending().await,
                }
            }, if refresh.is_some() => {
                refresh = None;
                let (started, deferred, refreshed) = result.map_err(|error| crate::error::AppError::Task(error.to_string()))?;
                // A refresh started before a queue switch or action must never
                // replace newer UI state with an obsolete snapshot.
                if started != generation || deferred != view.deferred_view || view.composer.is_some() || view.busy {
                    continue;
                }
                match refreshed {
                    Ok((requests, notices)) => {
                        let recovered = view.poll_error.take().is_some();
                        let changed_requests = view.update(requests);
                        let changed_notices = view.update_notices(notices);
                        if recovered || changed_requests || changed_notices { display(&view)?; }
                    }
                    Err(error) => {
                        let message = format!("Service unavailable: {error}; retrying");
                        if view.poll_error.as_ref() != Some(&message) {
                            view.poll_error = Some(message);
                            display(&view)?;
                        }
                    }
                }
            }
            result = async {
                match submission.as_mut() {
                    Some(task) => task.await,
                    None => std::future::pending().await,
                }
            }, if submission.is_some() => {
                submission = None;
                view.busy = false;
                let (is_instruction, result) = result.map_err(|error| crate::error::AppError::Task(error.to_string()))?;
                if is_instruction {
                    match result {
                        Ok(response) => { view.composer = None; view.status = Some(format!("Instruction accepted: {response}")); }
                        Err(error) => { if let Some(composer) = &mut view.composer { composer.error = Some(error.to_string()); } }
                    }
                } else {
                    match result {
                        Ok(_) => { view.status = None; view.update(Vec::new()); view.update_notices(Vec::new()); }
                        Err(error) => { view.status = Some(format!("Not confirmed: {error}; check state before retry")); }
                    }
                }
                display(&view)?;
                interval.reset_immediately();
            }
            key = receiver.recv() => {
                let Some(event) = key else { return Ok(()); };
                let key = match event? {
                    Event::Key(key) => key,
                    Event::Resize(_, _) => { display(&view)?; continue; },
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
                    Action::Redraw => display(&view)?,
                    Action::Quit => return Ok(()),
                    Action::ToggleDeferred => {
                        generation = generation.wrapping_add(1);
                        view.deferred_view = !view.deferred_view;
                        view.update(Vec::new());
                        display(&view)?;
                        interval.reset_immediately();
                    },
                    Action::SendInstruction(message) => {
                        generation = generation.wrapping_add(1);
                        if let Some(composer) = &mut view.composer { composer.error = None; }
                        view.busy = true;
                        display(&view)?;
                        let address = address.to_owned();
                        submission = Some(tokio::spawn(async move {
                            (true, network::call(&address, &Operation::SendMessage { message }).await)
                        }));
                    },
                    Action::Submit(operation) => {
                        generation = generation.wrapping_add(1);
                        view.status = None;
                        view.busy = true;
                        display(&view)?;
                        let address = address.to_owned();
                        submission = Some(tokio::spawn(async move {
                            (false, network::call(&address, &operation).await)
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
fn notice_pages(item: &NoticeItem) -> Vec<String> {
    let content = format!(
        "Work: {}   Result: {}   Activity: {}",
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
fn display(view: &View) -> Result<()> {
    print!("\x1b[2J\x1b[H");
    if view.busy {
        println!("Sending; outcome unknown until confirmed. Ctrl-C quits.\r");
    }
    if let Some(composer) = &view.composer {
        let labels = [
            "Retry identity (unique deployment-wide)",
            "Channel (must include operator)",
            "Recipient label (empty means group)",
            "Instruction text",
        ];
        if composer.stage < 4 {
            println!(
                "Compose instruction: {}\r\n{}\r\n\r\nEnter continues; Esc cancels.",
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
            println!("Review instruction, page {}/{}\r\n{}\r\n\r\nj/k pages; Enter sends or retries unchanged; Esc cancels.", page + 1, pages.len(), pages.get(page).map(String::as_str).unwrap_or(""));
            if let Some(error) = &composer.error {
                println!("\r\nSend failed or outcome unknown: {}\r\nRetry with the same identity, or retain it before cancelling.", safe(error));
            }
        }
        io::stdout().flush()?;
        return Ok(());
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
        io::stdout().flush()?;
        return Ok(());
    }
    if view.notice_view {
        if let Some(notice) = view.notices.get(view.notice_current) {
            let pages = notice_pages(notice);
            let page = view.notice_page.min(pages.len().saturating_sub(1));
            println!("Completion notice {}/{} from {} | page {}/{}\r\n\r\n{}\r\n\r\nh/l notices; j/k pages; d dismiss; n or q returns.", view.notice_current + 1, view.notices.len(), safe(&notice.notice.agent), page + 1, pages.len(), pages.get(page).map(String::as_str).unwrap_or(""));
        } else {
            println!("No open completion notices. n or q returns.");
        }
        io::stdout().flush()?;
        return Ok(());
    }
    println!(
        "Open completion notices: {} (n to view); m composes instruction\r",
        view.notices.len()
    );
    if let Some(error) = &view.poll_error {
        println!("{}\r", safe(error));
    }
    if let Some(status) = &view.status {
        println!("{}\r", safe(status));
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
            io::stdout().flush()?;
            return Ok(());
        }
        println!(
            "{} queue: {}/{} | importance {}\r\n{}\r\n{}\r\n",
            if view.deferred_view {
                "Deferred"
            } else {
                "Active"
            },
            view.current + 1,
            view.requests.len(),
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
                        format!(
                            "{} {}. [recommended] {}",
                            selected_mark,
                            position + 1,
                            option
                        )
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
            println!("\r\nj/k select; h/l requests; 1-9 select; Enter confirms; d defer/restore; p priority; c context; o answer detail; Tab queues; q quits.");
        }
    } else {
        println!(
            "No {} requests. Tab switches queues; q quits.",
            if view.deferred_view {
                "deferred"
            } else {
                "active"
            }
        );
    }
    io::stdout().flush()?;
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
    fn controls_from_remote_content_are_not_emitted() {
        assert_eq!(safe("a\u{1b}[2J\nb"), "a [2J b");
    }
}
