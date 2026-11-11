use std::cell::RefCell;
use std::rc::Rc;

use crate::logs;

type Sink = Box<dyn Fn(String)>;

thread_local! {
    static SINK: RefCell<Option<Sink>> = const { RefCell::new(None) };
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

fn emit(line: String, is_error: bool) {
    logs::line(&line);
    SINK.with(|sink| match &*sink.borrow() {
        Some(sink) => sink(line),
        None if is_error => eprintln!("{line}"),
        None => println!("{line}"),
    });
}
