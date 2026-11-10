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
    selected: Option<usize>,
    editing: bool,
    custom: String,
}
impl View {
    fn new() -> Self {
        Self {
            requests: Vec::new(),
            selected: None,
            editing: false,
            custom: String::new(),
        }
    }
    fn update(&mut self, requests: Vec<PendingRequest>) -> bool {
        if self.requests == requests {
            return false;
        }
        if self.requests.first().map(|r| r.id) != requests.first().map(|r| r.id) {
            self.selected = requests
                .first()
                .and_then(|r| r.request.recommendation.map(|_| 0));
            self.editing = false;
            self.custom.clear();
        }
        self.requests = requests;
        true
    }
    // Presentation position maps back to the original suggestion index; custom is last.
    fn choices(&self) -> Vec<usize> {
        let Some(pending) = self.requests.first() else {
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
        if self.editing {
            match key.code {
                KeyCode::Esc => self.editing = false,
                KeyCode::Backspace => {
                    self.custom.pop();
                }
                KeyCode::Enter if !self.custom.is_empty() => {
                    if let Some(request) = self.requests.first() {
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
        let choices = self.choices();
        let Some(request) = self.requests.first() else {
            return if key.code == KeyCode::Char('q') {
                Action::Quit
            } else {
                Action::None
            };
        };
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => Action::Quit,
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
                let response = network::call(address, &Operation::List).await?;
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
                    Action::Submit(operation) => {
                        network::call(address, &operation).await?;
                        view.update(Vec::new());
                        display(&view)?;
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
    if let Some(pending) = view.requests.first() {
        let request = &pending.request;
        println!(
            "Request {} from {}\r\n{}\r\n",
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
        if view.editing {
            println!(
                "\r\nCustom answer: {}_\r\nEnter submits; Esc cancels editing.",
                safe(&view.custom)
            );
        } else {
            println!("\r\nj/k or arrows select; 1-9 select; Enter confirms; q quits.");
        }
    } else {
        println!("No pending requests. q quits.");
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
            request: DecisionRequest {
                identity: "test".into(),
                agent: "agent".into(),
                question: "Choose".into(),
                options: (0..count).map(|i| format!("option {i}")).collect(),
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
