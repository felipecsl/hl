use colored::*;
use std::sync::atomic::{AtomicBool, Ordering};

static VERBOSE: AtomicBool = AtomicBool::new(false);

pub fn set_verbose(enabled: bool) {
  VERBOSE.store(enabled, Ordering::Relaxed);
}

pub fn is_verbose() -> bool {
  VERBOSE.load(Ordering::Relaxed)
}

pub fn debug(msg: &str) {
  if is_verbose() {
    eprintln!("{} {}", "›".bright_black(), msg.dimmed());
  }
}

pub fn log(msg: &str) {
  println!("{} {}", "•".bright_black(), msg);
}

pub fn ok(msg: &str) {
  println!("{} {}", "✓".green(), msg.bold());
}

#[allow(dead_code)]
pub fn warn(msg: &str) {
  println!("{} {}", "!".yellow(), msg);
}

#[allow(dead_code)]
pub fn err(msg: &str) {
  eprintln!("{} {}", "x".red(), msg);
}
