use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::logs;

type Sink = Box<dyn Fn(String)>;

thread_local! {
    static SINK: RefCell<Option<Sink>> = const { RefCell::new(None) };
}

/// The menu owns the screen: a line printed by a background thread (loading the list,
/// retrying the VPS) would land in the middle of it. Such lines go only to the log
static SCREEN_TAKEN: AtomicBool = AtomicBool::new(false);

pub fn take_screen(taken: bool) {
    SCREEN_TAKEN.store(taken, Ordering::Relaxed);
}

pub fn capture(sink: impl Fn(String) + 'static) {
    SINK.with(|current| *current.borrow_mut() = Some(Box::new(sink)));
}

pub fn step(title: &str) {
    emit(format!("==> {title}"), false);
}

pub fn info(text: &str) {
    emit(text.to_owned(), false);
}

pub fn warning(message: &str) {
    emit(format!("warning: {message}"), true);
}

pub fn collect<R>(run: impl FnOnce() -> R) -> (R, Vec<String>) {
    let lines = Rc::new(RefCell::new(Vec::new()));
    let sink = Rc::clone(&lines);
    let collector: Sink = Box::new(move |line| sink.borrow_mut().push(line));
    let previous = SINK.with(|current| current.borrow_mut().replace(collector));
    let result = run();
    SINK.with(|current| *current.borrow_mut() = previous);
    let collected = std::mem::take(&mut *lines.borrow_mut());
    (result, collected)
}

pub fn printable(text: &str) -> String {
    text.chars()
        .map(|character| match character {
            '\n' | '\t' => character,
            control if control.is_control() => '?',
            other => other,
        })
        .collect()
}

pub fn printable_line(text: &str) -> String {
    text.chars()
        .map(|character| {
            if character.is_control() {
                '?'
            } else {
                character
            }
        })
        .collect()
}

fn emit(line: String, is_error: bool) {
    let line = printable(&line);
    logs::line(&line);
    SINK.with(|sink| match &*sink.borrow() {
        Some(sink) => sink(line),
        None if SCREEN_TAKEN.load(Ordering::Relaxed) => {}
        None if is_error => eprintln!("{line}"),
        None => println!("{line}"),
    });
}

#[cfg(test)]
mod tests {
    use super::{printable, printable_line};

    #[test]
    fn removes_terminal_control_sequences() {
        assert_eq!(
            printable("a\u{1b}]52;c;QQ==\u{7}b\u{9b}c"),
            "a?]52;c;QQ==?b?c"
        );
        assert_eq!(printable("one\ttwo\nthree\r"), "one\ttwo\nthree?");
        assert_eq!(printable_line("a\nb\tc"), "a?b?c");
        assert_eq!(printable("файл.txt"), "файл.txt");
    }
}
