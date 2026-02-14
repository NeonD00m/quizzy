use crossterm::{
    ExecutableCommand, QueueableCommand, cursor,
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers, read},
    style::{Color, Print, SetForegroundColor},
    terminal::{Clear, ClearType, disable_raw_mode, enable_raw_mode},
};
use std::io::{Write, stdout};
use std::time::{Duration, Instant};

// super smart data structure to prevent program crash
// from leaving terminal in raw mode (and breaking it)
pub struct RawModeGuard;

impl RawModeGuard {
    pub fn new() -> anyhow::Result<Self> {
        enable_raw_mode()?;
        Ok(Self) // return self to make sure value not dropped until desired
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
    }
}

pub fn cards_input() -> KeyCode {
    let _guard = RawModeGuard::new();
    while let Ok(event) = read() {
        let Some(event) = event.as_key_press_event() else {
            continue;
        };
        if event.modifiers == KeyModifiers::CONTROL
            && (event.code == KeyCode::Char('c') || event.code == KeyCode::Char('d'))
        {
            return KeyCode::Esc;
        }
        if event.modifiers != KeyModifiers::NONE {
            println!("Ignoring input due to mofidier {:}\r", event.modifiers);
            continue;
        }
        if matches!(
            event.code,
            KeyCode::Esc | KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Left | KeyCode::Right
        ) {
            return event.code;
        }
    }
    KeyCode::Esc
}

pub fn choice_input() -> anyhow::Result<KeyCode> {
    let _guard = RawModeGuard::new();
    while let Ok(event) = read() {
        let Some(event) = event.as_key_press_event() else {
            continue;
        };
        if event.modifiers == KeyModifiers::CONTROL
            && (event.code == KeyCode::Char('c') || event.code == KeyCode::Char('d'))
        {
            return Ok(KeyCode::Esc);
        }
        if event.modifiers != KeyModifiers::NONE {
            println!("Ignoring input due to mofidier {:}\r", event.modifiers);
            continue;
        }
        if matches!(
            event.code,
            KeyCode::Esc
                | KeyCode::Char('1')
                | KeyCode::Char('2')
                | KeyCode::Char('3')
                | KeyCode::Char('4')
        ) {
            return Ok(event.code);
        }
    }
    Ok(KeyCode::Esc)
}

pub fn enter_input() -> anyhow::Result<KeyCode> {
    let _guard = RawModeGuard::new();
    while let Ok(event) = read() {
        let Some(event) = event.as_key_press_event() else {
            continue;
        };
        if event.modifiers == KeyModifiers::CONTROL
            && (event.code == KeyCode::Char('c') || event.code == KeyCode::Char('d'))
        {
            return Ok(KeyCode::Esc);
        }
        if matches!(event.code, KeyCode::Esc | KeyCode::Enter) {
            return Ok(event.code);
        }
    }
    Ok(KeyCode::Esc)
}

pub enum RoundAction {
    Answer(char), // '1', '2', '3', '4'
    Double,       // User typed "DOUBLE"
    Bank,         // User typed "BANK"
    Timeout,      // Time ran out
    Exit,         // User hit ESC
}

pub fn read_input_with_fuse(allowed_seconds: u64, prefix: &str) -> anyhow::Result<RoundAction> {
    let input_prefix = format!("{}> ", prefix);
    let mut stdout = stdout();
    let start_time = Instant::now();
    let duration = Duration::from_secs(allowed_seconds);

    // current typed input
    let mut input_buffer = String::new();

    // drain input buffer before starting loop
    while event::poll(Duration::from_millis(0))? {
        event::read()?;
    }

    let _guard = RawModeGuard::new();
    stdout.execute(cursor::Show)?;
    loop {
        let elapsed = start_time.elapsed();
        if elapsed >= duration {
            return Ok(RoundAction::Timeout);
        }

        // FUSE TIMER
        let remaining_secs = (duration - elapsed).as_secs_f32();

        let total_chars = (allowed_seconds as usize) * 3;
        let percent_left = remaining_secs / (allowed_seconds as f32);
        let chars_to_show = (total_chars as f32 * percent_left).ceil() as usize;

        let full_pattern = "--|".repeat(allowed_seconds as usize);
        let visible_fuse: String = full_pattern.chars().take(chars_to_show).collect();

        let color = if percent_left > 0.5 {
            Color::Green
        } else if percent_left > 0.25 {
            Color::Yellow
        } else {
            Color::Red
        };

        // draw timer line
        stdout
            .queue(cursor::MoveToColumn(0))?
            .queue(Clear(ClearType::CurrentLine))?
            .queue(Print("Time: ["))?
            .queue(SetForegroundColor(color))?
            .queue(Print(visible_fuse))?
            .queue(SetForegroundColor(Color::Reset))?
            .queue(Print("]"))?;

        // RENDER INPUT LINE
        stdout
            .queue(cursor::MoveDown(1))?
            .queue(cursor::MoveToColumn(0))?
            .queue(Clear(ClearType::CurrentLine))?
            .queue(Print(&input_prefix))?
            .queue(Print(&input_buffer))?
            .flush()?;

        // NON-BLOCKING INPUT
        if event::poll(Duration::from_millis(50))?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            match key.code {
                KeyCode::Backspace => {
                    input_buffer.pop();
                }
                KeyCode::Enter => {
                    let cmd = input_buffer.trim().to_uppercase();
                    if cmd == "DOUBLE" {
                        stdout.queue(cursor::MoveUp(1))?;
                        return Ok(RoundAction::Double);
                    } else if cmd == "BANK" {
                        stdout.queue(cursor::MoveUp(1))?;
                        return Ok(RoundAction::Bank);
                    } else {
                        input_buffer.clear()
                    }
                }
                KeyCode::Esc => {
                    stdout.queue(cursor::MoveUp(1))?;
                    return Ok(RoundAction::Exit);
                }
                KeyCode::Char(c) => {
                    // if empty and typed number we instantly
                    // submit that as the user's answer
                    if input_buffer.is_empty() && "1234".contains(c) {
                        stdout.queue(cursor::MoveUp(1))?;
                        return Ok(RoundAction::Answer(c));
                    }
                    input_buffer.push(c);
                }
                _ => {}
            }
        }
        stdout.queue(cursor::MoveUp(1))?;
    }
}

pub fn type_input(prefix: &str) -> anyhow::Result<Option<String>> {
    let input_prefix = format!("{}> ", prefix);
    let mut stdout = stdout();
    let mut input_buffer = String::new();

    // drain input buffer before starting loop
    while event::poll(Duration::from_millis(0))? {
        event::read()?;
    }

    let _guard = RawModeGuard::new();
    stdout.execute(cursor::Show)?;
    loop {
        // RENDER INPUT LINE
        stdout
            .queue(cursor::MoveDown(1))?
            .queue(cursor::MoveToColumn(0))?
            .queue(Clear(ClearType::CurrentLine))?
            .queue(Print(&input_prefix))?
            .queue(Print(&input_buffer))?
            .flush()?;

        // NON-BLOCKING INPUT
        if event::poll(Duration::from_millis(50))?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            match key.code {
                KeyCode::Backspace => {
                    input_buffer.pop();
                }
                KeyCode::Enter => {
                    stdout.execute(cursor::MoveUp(1))?;
                    return Ok(Some(input_buffer.trim().to_string()));
                }
                KeyCode::Esc => {
                    stdout.execute(cursor::MoveUp(1))?;
                    return Ok(None);
                }
                KeyCode::Char(c) => input_buffer.push(c),
                _ => {}
            };
        }
        stdout.queue(cursor::MoveUp(1))?;
    }
}
