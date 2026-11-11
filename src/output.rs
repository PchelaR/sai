use std::cell::RefCell;

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

fn emit(line: String, is_error: bool) {
    SINK.with(|sink| match &*sink.borrow() {
        Some(sink) => sink(line),
        None if is_error => eprintln!("{line}"),
        None => println!("{line}"),
    });
}
