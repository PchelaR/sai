use std::fs;

use ratatui::crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use super::{Explorer, Step, unique};

#[test]
fn picks_unique_folder_names() {
    let taken = vec!["src".to_owned(), "src-2".to_owned()];
    assert_eq!(unique("app", &taken), "app");
    assert_eq!(unique("src", &taken), "src-3");
}

#[test]
fn checks_folders_and_their_content() {
    let root = std::env::temp_dir().join(format!("sai-explorer-{}", std::process::id()));
    fs::create_dir_all(root.join("front/src")).unwrap();
    fs::create_dir_all(root.join("front/data")).unwrap();
    fs::create_dir_all(root.join("back/.git")).unwrap();
    fs::write(root.join("front/src/app.ts"), [0u8; 10]).unwrap();
    fs::write(root.join("front/data/big.bin"), [0u8; 90]).unwrap();
    let root = fs::canonicalize(&root).unwrap();

    let mut explorer = Explorer::open("shop-test-explorer");
    explorer.roots = vec![root.clone()];
    explorer.expanded.clear();
    explorer.expand(&root);
    explorer.cursor = 0;
    let names: Vec<String> = explorer.rows().iter().map(|row| row.name.clone()).collect();
    assert_eq!(names[1..], ["back", "front"]);
    assert_eq!(explorer.rows()[1].note, "git");

    explorer.key(KeyCode::Down);
    explorer.key(KeyCode::Down);
    explorer.key(KeyCode::Char(' '));
    for _ in 0..200 {
        explorer.tick();
        if explorer.uploaded() == 100 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(explorer.uploaded(), 100);
    explorer.key(KeyCode::Right);
    let names: Vec<String> = explorer.rows().iter().map(|row| row.name.clone()).collect();
    assert_eq!(names[3..], ["data", "src"]);
    explorer.key(KeyCode::Down);
    explorer.key(KeyCode::Char(' '));
    assert_eq!(explorer.uploaded(), 10);

    explorer.cursor = 0;
    explorer.key(KeyCode::Char(' '));
    assert!(explorer.message.is_some());

    explorer.area.set(Rect::new(0, 0, 60, 10));
    explorer.offset.set(0);
    let click = |column, row| MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    };
    explorer.mouse(click(3 + 2, 2));
    assert_eq!(explorer.cursor, 1);
    assert_eq!(explorer.rows().len(), 5);
    explorer.mouse(click(3 + 6, 4));
    assert_eq!(explorer.uploaded(), 100);
    explorer.mouse(click(3 + 6, 4));
    assert_eq!(explorer.uploaded(), 10);

    let Step::Apply(plan) = explorer.key(KeyCode::Enter) else {
        panic!("expected a plan");
    };
    assert_eq!(plan.add.len(), 1);
    assert_eq!(plan.add[0].alias, "front");
    assert_eq!(plan.add[0].selection.skip, ["data"]);
    assert_eq!(plan.add[0].selection.uploaded, 10);
    fs::remove_dir_all(root).unwrap();
}
