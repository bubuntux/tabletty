//! Asking the terminal what it can draw, without leaving anything behind.
//!
//! ratatui-image has its own probe, but it reads stdin from a background thread: if
//! the terminal never answers, the probe gives up after two seconds while the thread
//! stays blocked on stdin, then swallows the next keypress and switches raw mode off
//! under us. So we send ratatui-image's queries and parse the replies with its
//! parser, but do the reading ourselves: polled, with a deadline, on this thread.
//! Anything short of a complete answer means half-blocks.

use ratatui_image::picker::Picker;

/// Work out the best image protocol and cell size. Needs raw mode, and must run
/// before anything else reads stdin.
#[cfg(unix)]
pub fn detect_graphics() -> Picker {
    use std::io::Write;
    use std::time::{Duration, Instant};

    use ratatui_image::FontSize;
    use ratatui_image::picker::ProtocolType;
    use ratatui_image::picker::cap_parser::{Parser, QueryStdioOptions, Response};
    use rustix::event::{PollFd, PollFlags, Timespec, poll};

    /// Local terminals answer in a few milliseconds; this leaves room for SSH.
    const REPLY_TIMEOUT: Duration = Duration::from_millis(300);
    /// What most terminals use, for when the cell size can't be learned.
    const FALLBACK_CELL: (u16, u16) = (10, 20);

    let is_tmux = std::env::var("TERM").is_ok_and(|term| term.starts_with("tmux"))
        || std::env::var("TERM_PROGRAM").is_ok_and(|program| program == "tmux");
    let query = Parser::query(is_tmux, QueryStdioOptions::default());
    let mut stdout = std::io::stdout();
    if stdout
        .write_all(query.as_bytes())
        .and_then(|()| stdout.flush())
        .is_err()
    {
        return Picker::halfblocks();
    }

    // Read straight from the file descriptor: std's buffered stdin would keep bytes
    // that crossterm needs to see later.
    let stdin = rustix::stdio::stdin();
    let deadline = Instant::now() + REPLY_TIMEOUT;
    let mut parser = Parser::new();
    let mut responses = Vec::new();
    'reading: loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let timeout = Timespec {
            tv_sec: 0,
            tv_nsec: remaining.as_nanos() as _,
        };
        let mut fds = [PollFd::new(&stdin, PollFlags::IN)];
        if remaining.is_zero() || !matches!(poll(&mut fds, Some(&timeout)), Ok(ready) if ready > 0)
        {
            tracing::info!("terminal did not answer the graphics query; using half-blocks");
            return Picker::halfblocks();
        }
        let mut buffer = [0_u8; 64];
        let Ok(read) = rustix::io::read(stdin, &mut buffer) else {
            return Picker::halfblocks();
        };
        for &byte in &buffer[..read] {
            let parsed = parser.push(char::from(byte));
            // The status report is the last reply the query asks for.
            if parsed == [Response::Status] {
                break 'reading;
            }
            responses.extend(parsed);
        }
    }

    let protocol = if responses.contains(&Response::Kitty) {
        Some(ProtocolType::Kitty)
    } else if responses.contains(&Response::Sixel) {
        Some(ProtocolType::Sixel)
    } else {
        None
    };
    let cell = responses
        .iter()
        .find_map(|response| match response {
            Response::CellSize(size) => *size,
            _ => None,
        })
        .unwrap_or(FALLBACK_CELL);
    // The only constructor that takes a cell size. It also picks iTerm2 from the
    // environment where that applies, which a detected Kitty or Sixel overrides.
    #[allow(deprecated)]
    let mut picker = Picker::from_fontsize(FontSize::from(cell));
    if let Some(protocol) = protocol {
        picker.set_protocol_type(protocol);
    }
    tracing::info!(protocol = ?picker.protocol_type(), ?cell, "terminal graphics detected");
    picker
}

/// Windows consoles have no stdin poll to probe with safely; half-blocks work there.
#[cfg(not(unix))]
pub fn detect_graphics() -> Picker {
    Picker::halfblocks()
}
