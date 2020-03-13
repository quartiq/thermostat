use core::ops::Deref;
use super::command_parser::{Command, Error as ParserError};

const MAX_LINE_LEN: usize = 64;

struct LineReader {
    buf: [u8; MAX_LINE_LEN],
    pos: usize,
}

impl LineReader {
    pub fn new() -> Self {
        LineReader {
            buf: [0; MAX_LINE_LEN],
            pos: 0,
        }
    }

    pub fn feed(&mut self, c: u8) -> Option<LineResult> {
        if c == 13 || c == 10 {
            // Enter
            if self.pos > 0 {
                let len = self.pos;
                self.pos = 0;
                Some(LineResult {
                    buf: self.buf.clone(),
                    len,
                })
            } else {
                None
            }
        } else if self.pos < self.buf.len() {
            // Add input
            self.buf[self.pos] = c;
            self.pos += 1;
            None
        } else {
            // Buffer is full, ignore
            None
        }
    }
}

pub struct LineResult {
    buf: [u8; MAX_LINE_LEN],
    len: usize,
}

impl Deref for LineResult {
    type Target = [u8];
    fn deref(&self) -> &Self::Target {
        &self.buf[..self.len]
    }
}

pub enum SessionOutput {
    Nothing,
    Command(Command),
    Error(ParserError),
}

impl From<Result<Command, ParserError>> for SessionOutput {
    fn from(input: Result<Command, ParserError>) -> Self {
        input.map(SessionOutput::Command)
            .unwrap_or_else(SessionOutput::Error)
    }
}

pub const CHANNELS: usize = 2;

pub struct Session {
    reader: LineReader,
    reporting: bool,
    report_pending: [bool; CHANNELS],
}

impl Default for Session {
    fn default() -> Self {
        Session::new()
    }
}

impl Session {
    pub fn new() -> Self {
        Session {
            reader: LineReader::new(),
            reporting: false,
            report_pending: [false; CHANNELS],
        }
    }

    pub fn reset(&mut self) {
        self.reader = LineReader::new();
        self.reporting = false;
        self.report_pending = [false; CHANNELS];
    }

    pub fn is_dirty(&self) -> bool {
        self.reader.pos > 0
    }

    pub fn reporting(&self) -> bool {
        self.reporting
    }

    pub fn set_report_pending(&mut self, channel: usize) {
        if self.reporting {
            self.report_pending[channel] = true;
        }
    }

    pub fn is_report_pending(&self) -> Option<usize> {
        if ! self.reporting {
            None
        } else {
            self.report_pending.iter()
                .enumerate()
                .fold(None, |result, (channel, report_pending)| {
                    result.or_else(|| {
                        if *report_pending { Some(channel) } else { None }
                    })
                })
        }
    }

    pub fn mark_report_sent(&mut self, channel: usize) {
        self.report_pending[channel] = false;
    }

    pub fn feed(&mut self, buf: &[u8]) -> (usize, SessionOutput) {
        let mut buf_bytes = 0;
        for (i, b) in buf.iter().enumerate() {
            buf_bytes = i + 1;
            let line = self.reader.feed(*b);
            match line {
                Some(line) => {
                    let command = Command::parse(&line);
                    match command {
                        Ok(Command::Reporting(reporting)) => {
                            self.reporting = reporting;
                        }
                        _ => {}
                    }
                    return (buf_bytes, command.into());
                }
                None => {}
            }
        }
        (buf_bytes, SessionOutput::Nothing)
    }
}
