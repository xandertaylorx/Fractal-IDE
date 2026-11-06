use fractal_core::buffer::*;
use std::path::PathBuf;

fn buf(text: &str, ext: &str) -> Buffer {
    Buffer::from_text(text, Some(PathBuf::from(format!("x.{ext}"))))
}

#[test]
fn type_undo_redo() {
    let mut b = buf("", "txt");
    for c in "hello".chars() {
        b.type_char(c);
    }
    assert_eq!(b.text(), "hello");
    b.undo();
    assert_eq!(b.text(), "");
    b.redo();
    assert_eq!(b.text(), "hello");
}

#[test]
fn auto_close_and_overtype() {
    let mut b = buf("", "rs");
    b.type_char('(');
    assert_eq!(b.text(), "()");
    b.type_char(')');
    assert_eq!(b.text(), "()");
    assert_eq!(b.primary().head, 2);
}

#[test]
fn backspace_deletes_pair() {
    let mut b = buf("", "rs");
    b.type_char('{');
    b.backspace();
    assert_eq!(b.text(), "");
}

#[test]
fn newline_indents_in_braces() {
    let mut b = buf("fn a() {}", "rs");
    b.set_single_caret(8);
    b.newline();
    assert_eq!(b.text(), "fn a() {\n    \n}");
    assert_eq!(b.primary().head, 13);
}

#[test]
fn multi_cursor_typing() {
    let mut b = buf("a\nb\nc", "txt");
    b.sels = vec![Selection::caret(1), Selection::caret(3), Selection::caret(5)];
    b.insert_text("!", EditKind::Typing);
    assert_eq!(b.text(), "a!\nb!\nc!");
    b.undo();
    assert_eq!(b.text(), "a\nb\nc");
}

#[test]
fn move_lines_up_down() {
    let mut b = buf("one\ntwo\nthree", "txt");
    b.set_single_caret(5);
    b.move_lines(false);
    assert_eq!(b.text(), "one\nthree\ntwo");
    b.move_lines(true);
    assert_eq!(b.text(), "one\ntwo\nthree");
    b.undo();
    b.undo();
    assert_eq!(b.text(), "one\ntwo\nthree");
}

#[test]
fn copy_and_delete_lines() {
    let mut b = buf("a\nb", "txt");
    b.set_single_caret(0);
    b.copy_lines(false);
    assert_eq!(b.text(), "a\na\nb");
    b.delete_lines();
    assert_eq!(b.text(), "a\nb");
}

#[test]
fn toggle_comment_roundtrip() {
    let mut b = buf("  let x = 1;\n  let y = 2;", "rs");
    b.select_all();
    b.toggle_comment();
    assert_eq!(b.text(), "  // let x = 1;\n  // let y = 2;");
    b.toggle_comment();
    assert_eq!(b.text(), "  let x = 1;\n  let y = 2;");
}

#[test]
fn indent_outdent() {
    let mut b = buf("a\nb", "txt");
    b.select_all();
    b.indent_lines();
    assert_eq!(b.text(), "    a\n    b");
    b.outdent_lines();
    assert_eq!(b.text(), "a\nb");
}

#[test]
fn next_occurrence_and_replace() {
    let mut b = buf("foo bar foo", "txt");
    b.set_single_caret(1);
    b.add_next_occurrence();
    b.add_next_occurrence();
    assert_eq!(b.sels.len(), 2);
    b.insert_text("baz", EditKind::Other);
    assert_eq!(b.text(), "baz bar baz");
}

#[test]
fn surround_selection() {
    let mut b = buf("abc", "txt");
    b.set_selection(0, 3);
    b.type_char('"');
    assert_eq!(b.text(), "\"abc\"");
}

#[test]
fn motion_word_and_home() {
    let mut b = buf("    foo bar", "txt");
    b.set_single_caret(11);
    b.move_cursors(Motion::WordLeft, false);
    assert_eq!(b.primary().head, 8);
    b.move_cursors(Motion::LineStart, false);
    assert_eq!(b.primary().head, 4);
    b.move_cursors(Motion::LineStart, false);
    assert_eq!(b.primary().head, 0);
}

#[test]
fn crlf_roundtrip() {
    let b = buf("a\r\nb\r\n", "txt");
    assert!(b.crlf);
    assert_eq!(b.text(), "a\nb\n");
}

#[test]
fn find_replace() {
    let mut b = buf("Foo foo FOO", "txt");
    let q = fractal_core::search::SearchQuery { text: "foo".into(), ..Default::default() };
    assert_eq!(b.find_all(&q).len(), 3);
    assert_eq!(b.replace_matches(&q, "x", None), 3);
    assert_eq!(b.text(), "x x x");
}

#[test]
fn highlighter_runs() {
    use fractal_core::{syntax::Highlighter2, theme};
    let b = buf("fn main() { let s = \"hi\"; }\n", "rs");
    let th = theme::builtin("Fractal").unwrap();
    let mut h = Highlighter2::new(&b, &th);
    assert!(h.ensure(&b, 2, std::time::Duration::from_secs(1)));
    assert!(h.spans(0).unwrap().len() > 3);
    assert_eq!(h.syntax_name(), "Rust");
}

#[test]
fn vscode_theme_import() {
    let json = r##"{ // comment
      "name": "T", "type": "dark",
      "colors": { "editor.background": "#101010", "editor.foreground": "#eeeeee" },
      "tokenColors": [ { "scope": ["comment"], "settings": { "foreground": "#888888", "fontStyle": "italic" } } ],
    }"##;
    let t = fractal_core::theme::from_vscode_json(json, "x").unwrap();
    assert_eq!(t.ui.bg, [16, 16, 16, 255]);
}

#[test]
fn builtin_themes_all_build() {
    for n in fractal_core::theme::builtin_names() {
        assert!(fractal_core::theme::builtin(n).is_some(), "{n}");
    }
}

#[test]
fn vscode_default_themes_load_with_includes() {
    let ts = fractal_core::theme::vscode_defaults();
    assert_eq!(ts.len(), 4);
    let dm = ts.iter().find(|t| t.name == "Dark Modern").expect("Dark Modern");
    assert_eq!(dm.ui.bg, [0x1f, 0x1f, 0x1f, 255]);
    assert_eq!(dm.ui.bg_side, [0x18, 0x18, 0x18, 255]);
    assert_eq!(dm.ui.accent, [0x00, 0x78, 0xd4, 255]);
    // token colors inherited from dark_plus via include must exist
    assert!(dm.syntax.scopes.len() > 20);
    assert!(ts.iter().any(|t| t.name == "Light Modern" && !t.dark));
}

#[test]
fn html_auto_close_tag() {
    let mut b = buf("<div class=\"a\"", "html");
    b.set_single_caret(14);
    b.type_char('>');
    assert_eq!(b.text(), "<div class=\"a\"></div>");
    assert_eq!(b.primary().head, 15);
    let mut b = buf("<br", "html");
    b.set_single_caret(3);
    b.type_char('>');
    assert_eq!(b.text(), "<br>");
}

#[test]
fn quote_selected_word_and_markdown_emphasis() {
    let mut b = buf("say hello now", "txt");
    b.set_selection(4, 9);
    b.type_char('"');
    assert_eq!(b.text(), "say \"hello\" now");
    b.type_char('(');
    assert_eq!(b.text(), "say \"(hello)\" now");
    let mut m = buf("make bold", "md");
    m.set_selection(5, 9);
    m.type_char('*');
    m.type_char('*');
    assert_eq!(m.text(), "make **bold**");
}

#[test]
fn block_comment_and_completion_accept() {
    let mut b = buf("<p>hi</p>", "html");
    b.set_selection(0, 9);
    b.toggle_block_comment();
    assert_eq!(b.text(), "<!-- <p>hi</p> -->");
    b.toggle_block_comment();
    assert_eq!(b.text(), "<p>hi</p>");
    let mut r = buf("    ma", "rs");
    r.set_single_caret(6);
    let (p, start) = r.prefix_at_caret();
    assert_eq!(p, "ma");
    r.accept_completion(start, "fn main() {\n    $0\n}");
    assert_eq!(r.text(), "    fn main() {\n        \n    }");
    assert_eq!(r.primary().head, 4 + 12 + 8);
}
