//! The editor through its public surface.

#![cfg(feature = "edit")]
// `clippy.toml`'s `allow-unwrap-in-tests` does not reach integration tests.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use eeltex::edit::{Command, Cursor, Editor, Motion};

#[test]
fn a_frontend_can_drive_a_formula_with_nothing_else() {
    let mut editor = Editor::new();
    for ch in "x+1".chars() {
        editor.apply(Command::Insert(ch));
    }
    assert_eq!(editor.tex(), "x+1");

    // `/` takes the operand before it and leaves the caret below the bar.
    editor.apply(Command::Insert('/'));
    editor.apply(Command::Insert('2'));
    assert_eq!(editor.tex(), r"x+\frac12");

    editor.apply(Command::Move(Motion::Right));
    editor.apply(Command::Insert('='));
    assert_eq!(editor.tex(), r"x+\frac12=");

    assert!(editor.can_undo());
    editor.apply(Command::Undo);
    assert_eq!(editor.tex(), r"x+\frac12");
}

#[test]
fn a_formula_survives_being_opened_and_saved() {
    for source in [
        r"\frac{-b \pm \sqrt{b^2 - 4ac}}{2a}",
        r"\sum_{n=1}^{\infty} \frac{1}{n^2} = \frac{\pi^2}{6}",
        r"\begin{pmatrix} 1 & 2 \\ 3 & 4 \end{pmatrix}",
        r"\left( \frac{\partial f}{\partial x} \right)_{y}",
    ] {
        let editor = Editor::from_tex(source).expect("parses");
        let written = editor.tex();
        assert_eq!(
            eeltex::parse(&written).expect("re-parses"),
            eeltex::parse(source).expect("parses"),
            "{source} became {written}"
        );
    }
}

#[test]
fn nothing_a_frontend_can_do_breaks_the_formula() {
    let script = [
        Command::Insert('a'),
        Command::Insert('/'),
        Command::Insert('b'),
        Command::Insert('^'),
        Command::Insert('2'),
        Command::Move(Motion::Left),
        Command::Insert('('),
        Command::Insert(')'),
        Command::Backspace,
        Command::Backspace,
        Command::Move(Motion::Up),
        Command::Delete,
        Command::SelectAll,
        Command::Insert('z'),
        Command::Named("sqrt".into()),
        Command::Named("nonsense".into()),
        Command::Undo,
        Command::Redo,
        Command::Escape,
    ];
    let mut editor = Editor::new();
    for command in script {
        editor.apply(command.clone());
        let tex = editor.tex();
        assert!(
            eeltex::parse(&tex).is_ok(),
            "{command:?} produced {tex:?}, which does not parse"
        );
    }
}

#[test]
fn a_cursor_set_from_outside_is_made_safe() {
    use eeltex::edit::{SlotName, Step};
    let mut editor = Editor::from_tex("x").expect("parses");
    editor.set_cursor(Cursor::at(
        vec![Step::new(99, SlotName::Num), Step::new(0, SlotName::Den)],
        99,
    ));
    editor.apply(Command::Insert('y'));
    assert_eq!(editor.tex(), "xy");
}

#[cfg(feature = "embed-math-font")]
mod drawn {
    use super::*;
    use eeltex::{Fonts, Options};

    fn scene(editor: &Editor) -> eeltex::Scene {
        let fonts = Fonts::embedded().expect("the embedded font parses");
        editor
            .layout(&fonts, &Options::default().with_font_size(32.0))
            .expect("lays out")
    }

    #[test]
    fn the_caret_is_a_rule_the_scene_can_carry() {
        let editor = Editor::from_tex(r"\frac{a}{b}").expect("parses");
        let mut drawn = scene(&editor);
        let caret = editor.caret(&drawn).expect("a caret");
        drawn.rules.push(caret.into_rule());
        // The fraction bar and the caret.
        assert_eq!(drawn.rules.len(), 2);
    }

    #[test]
    fn an_empty_formula_still_draws_and_still_has_a_caret() {
        let editor = Editor::new();
        let drawn = scene(&editor);
        assert!(drawn.width > 0.0, "the empty placeholder has no width");
        assert!(editor.caret(&drawn).is_some());
    }

    #[test]
    fn an_edit_leaves_the_spans_of_what_it_did_not_touch_alone() {
        let mut editor = Editor::from_tex(r"\frac{abc}{xyz}").expect("parses");
        let before = scene(&editor);
        let spans_of = |scene: &eeltex::Scene| -> Vec<eeltex::Span> {
            scene
                .glyphs
                .iter()
                .filter_map(|g| scene.node(g.node()))
                .map(|n| n.span)
                .filter(|s| s.is_known())
                .collect()
        };
        let was = spans_of(&before);
        assert!(!was.is_empty(), "no glyph carried a span to begin with");

        editor.set_cursor(Cursor::at(Vec::new(), 0));
        editor.apply(Command::Insert('k'));
        let after = scene(&editor);
        let now = spans_of(&after);

        assert_eq!(
            after.glyphs.len(),
            before.glyphs.len() + 1,
            "an insertion should add exactly one glyph"
        );
        // Not a prefix check: the new glyph may sort anywhere among them.
        for span in &was {
            assert!(
                now.contains(span),
                "{span:?} was lost — an untouched glyph was renumbered by the edit"
            );
        }
    }
}
