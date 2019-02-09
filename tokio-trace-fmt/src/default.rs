use super::Context;


use tokio_trace_core::{
    Event, Level, field::{self, Field},
};

use std::{
    fmt,
    io::{self, Write},
};

#[cfg(feature = "ansi")]
use ansi_term::{Colour, Style};

pub fn fmt_event(ctx: Context, f: &mut Write, event: &Event) -> io::Result<()> {
    let meta = event.metadata();
    write!(f, "{} {}{}:", FmtLevel(meta.level()), FmtCtx(&ctx), meta.target())?;
    event.record(&mut RecordWriter(f));
    write!(f, "{}", FmtFields(&ctx));
    writeln!(f, "")
}

pub(crate) struct RecordWriter<'a>(&'a mut Write);

impl<'a> field::Record for RecordWriter<'a> {
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.record_debug(field, &format_args!("{}", value))
        } else {
            self.record_debug(field, &value)
        }
    }

    /// Record a value implementing `fmt::Debug`.
    fn record_debug(&mut self, field: &Field, value: &fmt::Debug) {
        if field.name() == "message" {
            let _ = write!(self.0, " {:?}", value);
        } else {
            let _ = write!(self.0, " {}={:?}", field, value);
        }
    }

}

struct FmtCtx<'a>(&'a Context<'a>);

#[cfg(feature = "ansi")]
impl<'a> fmt::Display for FmtCtx<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut seen = false;
        self.0.fmt_spans(|span| {
            if seen {
                write!(f, ":{}", Style::new().bold().paint(span))?;
            } else {
                write!(f, "{}", Style::new().bold().paint(span))?;
                seen = true;
            }
            Ok(())
        })?;
        if seen {
            f.pad(" ");
        }
        Ok(())
    }
}

#[cfg(not(feature = "ansi"))]
impl<'a> fmt::Display for FmtCtx<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut seen = false;
        self.0.fmt_spans(|span| {
            if seen {
                write!(f, ":{}", span)?;
            } else {
                write!(f, "{}", span)?;
                seen = true;
            }
            Ok(())
        })?;
        if seen {
            f.pad(" ");
        }
        Ok(())
    }
}

struct FmtFields<'a>(&'a Context<'a>);

impl<'a> fmt::Display for FmtFields<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt_fields(|fields| {
            write!(f, "{}", fields)
        })
    }
}

struct FmtLevel<'a>(&'a Level);

#[cfg(not(feature = "ansi"))]
impl<'a> fmt::Display for FmtLevel<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.0 {
            &Level::TRACE => f.pad("TRACE"),
            &Level::DEBUG => f.pad("DEBUG"),
            &Level::INFO => f.pad("INFO"),
            &Level::WARN => f.pad("WARN"),
            &Level::ERROR => f.pad("ERROR"),
        }
    }
}

#[cfg(feature = "ansi")]
impl<'a> fmt::Display for FmtLevel<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.0 {
            &Level::TRACE => write!(f, "{}", Colour::Purple.paint("TRACE")),
            &Level::DEBUG => write!(f, "{}", Colour::Blue.paint("DEBUG")),
            &Level::INFO => write!(f, "{}", Colour::Green.paint(" INFO")),
            &Level::WARN => write!(f, "{}", Colour::Yellow.paint(" WARN")),
            &Level::ERROR => write!(f, "{}", Colour::Red.paint("ERROR")),
        }
    }
}