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
    protocol::{Operation, PendingRequest, RequestList},
};

const POLL_INTERVAL: Duration = Duration::from_millis(500);

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

struct View {
    requests: Vec<PendingRequest>,
    current: usize,
    deferred_view: bool,
    priority_edit: bool,
    selected: Option<usize>,
    editing: bool,
    custom: String,
}
impl View {
    fn new() -> Self {
        Self {
            requests: Vec::new(),
            current: 0,
            deferred_view: false,
            priority_edit: false,
            selected: None,
            editing: false,
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
        self.priority_edit = false;
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
                    Ok(Event::Key(key)) => {
                        if sender.send(Ok(key)).is_err() {
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
    display(&view)?;
    loop {
        tokio::select! {
            _ = interval.tick() => {
                let operation = if view.deferred_view { Operation::ListDeferred } else { Operation::List };
                let response = network::call(address, &operation).await?;
                let requests: RequestList = serde_json::from_value(response)?;
                for request in &requests.requests { request.request.validate()?; }
                if view.update(requests.requests) { display(&view)?; }
            }
            key = receiver.recv() => {
                let Some(key) = key else { return Ok(()); };
                match view.key(key?) {
                    Action::None => {},
                    Action::Redraw => display(&view)?,
                    Action::Quit => return Ok(()),
                    Action::ToggleDeferred => {
                        view.deferred_view = !view.deferred_view;
                        view.update(Vec::new());
                        display(&view)?;
                        interval.reset_immediately();
                    },
                    Action::Submit(operation) => {
                        network::call(address, &operation).await?;
                        view.update(Vec::new());
                        display(&view)?;
                        interval.reset_immediately();
                    }
                }
            }
        }
    }
}

// Do not let remote content inject terminal control sequences.
fn safe(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}
fn display(view: &View) -> Result<()> {
    print!("\x1b[2J\x1b[H");
    if let Some(pending) = view.current() {
        let request = &pending.request;
        println!(
            "{} queue: {}/{} | importance {}\r\nRequest {} from {}\r\n{}\r\n",
            if view.deferred_view {
                "Deferred"
            } else {
                "Active"
            },
            view.current + 1,
            view.requests.len(),
            pending.importance,
            pending.id,
            safe(&request.agent),
            safe(&request.question)
        );
        let choices = view.choices();
        for (position, index) in choices.iter().enumerate() {
            if let Some(option) = request.options.get(*index) {
                let recommended = if Some(*index) == request.recommendation {
                    format!(
                        " (recommended: {})",
                        safe(request.reason.as_deref().unwrap_or(""))
                    )
                } else {
                    String::new()
                };
                println!(
                    "{} {}. {}{}\r",
                    if view.selected == Some(position) {
                        ">"
                    } else {
                        " "
                    },
                    position + 1,
                    safe(option),
                    recommended
                );
            }
        }
        println!(
            "{} {}. Custom answer\r",
            if view.selected == Some(choices.len()) {
                ">"
            } else {
                " "
            },
            choices.len() + 1
        );
        if view.priority_edit {
            println!("\r\nSet importance 0-5; Esc cancels.");
        } else if view.editing {
            println!(
                "\r\nCustom answer: {}_\r\nEnter submits; Esc cancels editing.",
                safe(&view.custom)
            );
        } else {
            println!("\r\nj/k select; h/l requests; 1-9 select; Enter confirms; d defer/restore; p priority; Tab queues; q quits.");
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
