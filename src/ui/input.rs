use crossterm::{
    ExecutableCommand, QueueableCommand, cursor,
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers, read},
    style::{Color, Print, SetForegroundColor},
    terminal::{Clear, ClearType, disable_raw_mode, enable_raw_mode},
};
use std::io::{IsTerminal, Write, stdout};
use std::time::{Duration, Instant};

// super smart data structure to prevent program crash
// from leaving terminal in raw mode (and breaking it)
pub struct RawModeGuard;

impl RawModeGuard {
    pub fn new() -> anyhow::Result<Self> {
        if std::io::stdin().is_terminal() {
            enable_raw_mode()?;
        }
        Ok(Self) // return self to make sure value not dropped until desired
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        if std::io::stdin().is_terminal() {
            let _ = disable_raw_mode();
        }
    }
}

pub fn cards_input() -> KeyCode {
    if !std::io::stdin().is_terminal() {
        let mut input = String::new();
        if std::io::stdin().read_line(&mut input).is_err() {
            return KeyCode::Esc;
        }
        let trimmed = input.trim().to_lowercase();
        if trimmed == "q" || trimmed == "exit" || trimmed == "quit" || trimmed == "esc" {
            return KeyCode::Esc;
        }
        if trimmed == "a" || trimmed == "left" || trimmed == "prev" {
            return KeyCode::Left;
        }
        if trimmed == "d" || trimmed == "right" || trimmed == "next" {
            return KeyCode::Right;
        }
        if trimmed == " " || trimmed == "space" || trimmed == "flip" {
            return KeyCode::Char(' ');
        }
        return KeyCode::Enter;
    }

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
            KeyCode::Esc
                | KeyCode::Enter
                | KeyCode::Char(' ')
                | KeyCode::Left
                | KeyCode::Right
                | KeyCode::Char('a')
                | KeyCode::Char('A')
                | KeyCode::Char('d')
                | KeyCode::Char('D')
        ) {
            return event.code;
        }
    }
    KeyCode::Esc
}

pub fn choice_input() -> anyhow::Result<KeyCode> {
    if !std::io::stdin().is_terminal() {
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        let trimmed = input.trim();
        if trimmed == "q" || trimmed == "exit" || trimmed == "esc" {
            return Ok(KeyCode::Esc);
        }
        if trimmed == "1" {
            return Ok(KeyCode::Char('1'));
        }
        if trimmed == "2" {
            return Ok(KeyCode::Char('2'));
        }
        if trimmed == "3" {
            return Ok(KeyCode::Char('3'));
        }
        if trimmed == "4" {
            return Ok(KeyCode::Char('4'));
        }
        return Ok(KeyCode::Esc);
    }

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

pub fn key_input(keycode: KeyCode) -> anyhow::Result<KeyCode> {
    if !std::io::stdin().is_terminal() {
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        let trimmed = input.trim().to_lowercase();
        if trimmed == "q" || trimmed == "exit" || trimmed == "quit" || trimmed == "esc" {
            return Ok(KeyCode::Esc);
        }
        return Ok(keycode);
    }

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
        if event.code == KeyCode::Esc || event.code == keycode {
            return Ok(event.code);
        }
    }
    Ok(KeyCode::Esc)
}

pub fn enter_input() -> anyhow::Result<KeyCode> {
    key_input(KeyCode::Enter)
}

pub enum RoundAction {
    Answer(char), // '1', '2', '3', '4'
    Double,       // User typed "DOUBLE"
    Bank,         // User typed "BANK"
    Timeout,      // Time ran out
    Exit,         // User hit ESC
}

pub fn read_input_with_fuse(allowed_seconds: u64, prefix: &str) -> anyhow::Result<RoundAction> {
    if !std::io::stdin().is_terminal() {
        print!("{}> ", prefix);
        stdout().flush()?;
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        let trimmed = input.trim().to_uppercase();
        if trimmed == "DOUBLE" {
            return Ok(RoundAction::Double);
        } else if trimmed == "BANK" {
            return Ok(RoundAction::Bank);
        } else if trimmed == "EXIT" || trimmed == "ESC" || trimmed == "Q" {
            return Ok(RoundAction::Exit);
        } else if trimmed.starts_with('1') {
            return Ok(RoundAction::Answer('1'));
        } else if trimmed.starts_with('2') {
            return Ok(RoundAction::Answer('2'));
        } else if trimmed.starts_with('3') {
            return Ok(RoundAction::Answer('3'));
        } else if trimmed.starts_with('4') {
            return Ok(RoundAction::Answer('4'));
        } else {
            return Ok(RoundAction::Exit);
        }
    }

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
    println!(); // make space for fuse line
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

/// `prefix` CANNOT HAVE new line characters
pub fn type_input(prefix: &str) -> anyhow::Result<Option<String>> {
    if !std::io::stdin().is_terminal() {
        print!("{}> ", prefix);
        stdout().flush()?;
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        let trimmed = input.trim();
        if trimmed.to_lowercase() == "esc" || trimmed.to_lowercase() == "exit" {
            return Ok(None);
        }
        return Ok(Some(trimmed.to_string()));
    }

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
            if key.modifiers == KeyModifiers::CONTROL
                && (key.code == KeyCode::Char('c') || key.code == KeyCode::Char('d'))
            {
                return Ok(None);
            }
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

pub enum StatsInput {
    Exit,
    Back,
    Index(u32),
    Up,
    Down,
    Confirm,
}

/// KeyCode::Esc signals quit, KeyCode::BackTab signals to go back
/// `prefix` CANNOT HAVE new line characters
pub fn stats_input(prefix: &str) -> anyhow::Result<StatsInput> {
    if !std::io::stdin().is_terminal() {
        print!("{}> ", prefix);
        stdout().flush()?;
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        let trimmed = input.trim().to_lowercase();
        if trimmed == "q" || trimmed == "exit" {
            return Ok(StatsInput::Exit);
        }
        if trimmed == "u" || trimmed == "up" {
            return Ok(StatsInput::Up);
        }
        if trimmed == "d" || trimmed == "down" {
            return Ok(StatsInput::Down);
        }
        if trimmed == "b" || trimmed == "back" || trimmed == "esc" {
            return Ok(StatsInput::Back);
        }
        if trimmed.is_empty() {
            return Ok(StatsInput::Confirm);
        }
        if let Ok(idx) = trimmed.parse::<u32>() {
            return Ok(StatsInput::Index(idx));
        }
        return Ok(StatsInput::Confirm);
    }

    let input_prefix = format!("{}> ", prefix);
    let mut stdout = stdout();
    let mut input_buffer: u32 = 0;
    let mut typed: u8 = 0;

    // drain input buffer before starting loop
    while event::poll(Duration::from_millis(0))? {
        event::read()?;
    }

    let _guard = RawModeGuard::new();
    stdout.execute(cursor::Show)?;
    loop {
        // RENDER INPUT LINE
        let s = input_buffer.to_string();
        stdout
            .queue(cursor::MoveDown(1))?
            .queue(cursor::MoveToColumn(0))?
            .queue(Clear(ClearType::CurrentLine))?
            .queue(Print(&input_prefix))?
            .queue(Print(if typed > 0 { &s } else { "" }))?
            .flush()?;

        // NON-BLOCKING INPUT
        if event::poll(Duration::from_millis(50))?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            if key.modifiers == KeyModifiers::CONTROL
                && (key.code == KeyCode::Char('c') || key.code == KeyCode::Char('d'))
            {
                stdout.execute(cursor::MoveUp(1))?;
                return Ok(StatsInput::Exit);
            }

            match key.code {
                KeyCode::Up => {
                    stdout.execute(cursor::MoveUp(1))?;
                    return Ok(StatsInput::Up);
                }
                KeyCode::Down => {
                    stdout.execute(cursor::MoveUp(1))?;
                    return Ok(StatsInput::Down);
                }
                KeyCode::Backspace => {
                    input_buffer /= 10;
                    typed = typed.saturating_sub(1);
                }
                KeyCode::Enter => {
                    stdout.execute(cursor::MoveUp(1))?;
                    if typed > 0 {
                        return Ok(StatsInput::Index(input_buffer));
                    } else {
                        return Ok(StatsInput::Confirm);
                    }
                }
                KeyCode::Esc => {
                    stdout.execute(cursor::MoveUp(1))?;
                    return Ok(StatsInput::Back);
                }
                KeyCode::Char('q') | KeyCode::Char('Q') => {
                    stdout.execute(cursor::MoveUp(1))?;
                    return Ok(StatsInput::Exit);
                }
                KeyCode::Char(c) => {
                    // only allow typing numbers 0-9 for indices
                    if let Some(d) = c.to_digit(10) {
                        input_buffer = input_buffer.saturating_mul(10).saturating_add(d);
                        typed += 1;
                    }
                }
                _ => {}
            };
        }
        stdout.queue(cursor::MoveUp(1))?;
    }
}
