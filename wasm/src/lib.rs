//! The demo page's interface to `eeltex::edit`.
//!
//! Strings cross as (pointer, length) pairs in linear memory. JavaScript writes input into a
//! buffer from [`alloc`]; each call that changes the formula re-renders, and [`out_ptr`] /
//! [`out_len`] then expose the SVG, a JSON report of the scene, or copied LaTeX.

use std::cell::RefCell;
use std::fmt::Write;

use eeltex::edit::{Command, Editor, Motion};
use eeltex::export::svg::{to_svg, SvgOptions};
use eeltex::{Fonts, NodeId, Options, Scene};

#[link(wasm_import_module = "env")]
extern "C" {
    /// `performance.now()`, in milliseconds.
    fn now() -> f64;
}

struct State {
    fonts: Fonts<'static>,
    editor: Editor,
    options: Options,
    scene: Scene,
    svg: String,
    info: String,
    clip: String,
    error: Option<String>,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
    static INPUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

fn with<R>(f: impl FnOnce(&mut State) -> R) -> Option<R> {
    STATE.with(|cell| cell.borrow_mut().as_mut().map(f))
}

fn input() -> String {
    INPUT.with(|buf| String::from_utf8_lossy(&buf.borrow()).into_owned())
}

/// A buffer of `len` bytes for the next string argument.
#[no_mangle]
pub extern "C" fn alloc(len: usize) -> *mut u8 {
    INPUT.with(|buf| {
        let mut buf = buf.borrow_mut();
        buf.clear();
        buf.resize(len, 0);
        buf.as_mut_ptr()
    })
}

/// Load the font and start with the formula in the input buffer. Returns 0 on failure.
#[no_mangle]
pub extern "C" fn init(font_size: f32) -> u32 {
    let Ok(fonts) = Fonts::embedded() else {
        return 0;
    };
    let editor = Editor::from_tex(&input()).unwrap_or_default();
    let mut state = State {
        fonts,
        editor,
        options: Options::default().with_font_size(font_size),
        scene: Scene::default(),
        svg: String::new(),
        info: String::new(),
        clip: String::new(),
        error: None,
    };
    render(&mut state);
    STATE.with(|cell| *cell.borrow_mut() = Some(state));
    1
}

/// Keys the page forwards. `shift` extends the selection; `word` moves by word.
#[no_mangle]
pub extern "C" fn key(code: u32, shift: u32, word: u32) -> u32 {
    let motion = |plain, by_word| if word != 0 { by_word } else { plain };
    let movement = match code {
        1 => Some(motion(Motion::Left, Motion::WordLeft)),
        2 => Some(motion(Motion::Right, Motion::WordRight)),
        3 => Some(Motion::Up),
        4 => Some(Motion::Down),
        5 => Some(Motion::Home),
        6 => Some(Motion::End),
        _ => None,
    };
    let command = match (movement, code) {
        (Some(m), _) if shift != 0 => Command::Extend(m),
        (Some(m), _) => Command::Move(m),
        (None, 7) => Command::Backspace,
        (None, 8) => Command::Delete,
        (None, 9) => Command::SelectAll,
        (None, 10) => Command::Undo,
        (None, 11) => Command::Redo,
        (None, 12) => Command::Escape,
        (None, 13) => Command::NewRow,
        (None, 14) => Command::Submit,
        _ => return 0,
    };
    apply(command)
}

/// One typed character.
#[no_mangle]
pub extern "C" fn insert(ch: u32) -> u32 {
    char::from_u32(ch).map_or(0, |ch| apply(Command::Insert(ch)))
}

/// Paste the input buffer.
#[no_mangle]
pub extern "C" fn paste() -> u32 {
    apply(Command::InsertText(input()))
}

/// Replace the formula with the input buffer. Returns 0 if it does not parse.
#[no_mangle]
pub extern "C" fn set_tex() -> u32 {
    let src = input();
    with(|s| match s.editor.set_tex(&src) {
        Ok(()) => {
            s.error = None;
            render(s);
            1
        }
        Err(e) => {
            s.error = Some(e.to_string());
            render(s);
            0
        }
    })
    .unwrap_or(0)
}

/// Place the caret at a point in scene space, or extend the selection to it.
#[no_mangle]
pub extern "C" fn click(x: f32, y: f32, shift: u32) -> u32 {
    with(|s| {
        let Some(cursor) = s.editor.cursor_at(&s.scene, x, y) else {
            return 0;
        };
        if shift != 0 {
            s.editor.extend_to(cursor);
        } else {
            s.editor.set_cursor(cursor);
        }
        render(s);
        1
    })
    .unwrap_or(0)
}

/// Select the sub-expression under a point.
#[no_mangle]
pub extern "C" fn select_at(x: f32, y: f32) -> u32 {
    with(|s| {
        let Some(node) = s.scene.hit(x, y) else {
            return 0;
        };
        let hit = s.editor.select_node(&s.scene, node);
        render(s);
        u32::from(hit)
    })
    .unwrap_or(0)
}

/// The innermost node under a point, or -1.
#[no_mangle]
pub extern "C" fn hit(x: f32, y: f32) -> i32 {
    with(|s| s.scene.hit(x, y).map_or(-1, |n| n.0 as i32)).unwrap_or(-1)
}

/// Put the selection's LaTeX in output 2, for a copy. Returns 0 with nothing selected.
#[no_mangle]
pub extern "C" fn copy() -> u32 {
    with(|s| match s.editor.selected_tex() {
        Some(tex) => {
            s.clip = tex;
            1
        }
        None => 0,
    })
    .unwrap_or(0)
}

fn output(s: &State, which: u32) -> &str {
    match which {
        0 => &s.svg,
        1 => &s.info,
        _ => &s.clip,
    }
}

/// The output strings: 0 the SVG, 1 the JSON report, 2 the copied LaTeX.
#[no_mangle]
pub extern "C" fn out_ptr(which: u32) -> *const u8 {
    with(|s| output(s, which).as_ptr()).unwrap_or(std::ptr::null())
}

#[no_mangle]
pub extern "C" fn out_len(which: u32) -> usize {
    with(|s| output(s, which).len()).unwrap_or(0)
}

fn apply(command: Command) -> u32 {
    with(|s| {
        let changed = s.editor.apply(command);
        if changed.is_anything() {
            s.error = None;
            render(s);
        }
        u32::from(changed.is_anything())
    })
    .unwrap_or(0)
}

fn svg_options() -> SvgOptions<'static> {
    SvgOptions::default().with_padding(8.0)
}

fn render(s: &mut State) {
    match s.editor.layout(&s.fonts, &s.options) {
        Ok(scene) => s.scene = scene,
        Err(e) => s.error = Some(e.to_string()),
    }
    match to_svg(&s.scene, &s.fonts, &svg_options()) {
        Ok(svg) => s.svg = svg,
        Err(e) => s.error = Some(e.to_string()),
    }
    s.info = report(s);
}

/// Milliseconds per call of `f`, repeated until `min_ms` has passed: browsers coarsen the
/// clock to as much as a millisecond, far longer than one layout.
fn per_call(min_ms: f64, mut f: impl FnMut()) -> f64 {
    // SAFETY: `now` is `performance.now`, which takes nothing and returns a number.
    let clock = || unsafe { now() };
    let start = clock();
    let mut runs = 0u32;
    loop {
        f();
        runs += 1;
        let elapsed = clock() - start;
        if elapsed >= min_ms || runs >= 100_000 {
            return elapsed / f64::from(runs);
        }
    }
}

/// Milliseconds to lay the current formula out (lowering included).
#[no_mangle]
pub extern "C" fn bench_layout(min_ms: f64) -> f64 {
    with(|s| per_call(min_ms, || drop(s.editor.layout(&s.fonts, &s.options)))).unwrap_or(0.0)
}

/// Milliseconds to write the current scene as SVG.
#[no_mangle]
pub extern "C" fn bench_svg(min_ms: f64) -> f64 {
    with(|s| per_call(min_ms, || drop(to_svg(&s.scene, &s.fonts, &svg_options())))).unwrap_or(0.0)
}

/// Everything the page draws or lists, as JSON.
fn report(s: &State) -> String {
    let scene = &s.scene;
    let editor = &s.editor;
    let mut out = String::with_capacity(256 + scene.nodes.len() * 96);
    let _ = write!(
        out,
        "{{\"tex\":{},\"error\":{},\"pending\":{},\"undo\":{},\"redo\":{},\
         \"svg_bytes\":{},\"glyphs\":{},\"rules\":{},\
         \"width\":{},\"ascent\":{},\"descent\":{},\"font_size\":{},\"missing\":{},",
        json_str(&editor.tex()),
        s.error.as_deref().map_or("null".into(), json_str),
        editor.pending().map_or("null".into(), json_str),
        editor.can_undo(),
        editor.can_redo(),
        s.svg.len(),
        scene.glyphs.len(),
        scene.rules.len(),
        num(scene.width),
        num(scene.ascent),
        num(scene.descent),
        num(scene.font_size),
        scene.missing.len(),
    );
    out.push_str("\"unknown\":[");
    for (i, name) in editor.unknown_commands().iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&json_str(name));
    }
    out.push_str("],\"caret\":");
    match editor.caret(scene) {
        Some(c) => {
            let _ = write!(
                out,
                "[{},{},{},{}]",
                num(c.x),
                num(c.y),
                num(c.thickness),
                num(c.height)
            );
        }
        None => out.push_str("null"),
    }
    out.push_str(",\"selection\":");
    match editor.selection().and_then(|sel| sel.bounds(editor, scene)) {
        Some([x, y, w, h]) => {
            let _ = write!(out, "[{},{},{},{}]", num(x), num(y), num(w), num(h));
        }
        None => out.push_str("null"),
    }
    out.push_str(",\"glyph_nodes\":[");
    for (i, g) in scene.glyphs.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(out, "{}", g.attrs.node.0);
    }
    out.push_str("],\"nodes\":[");
    for (i, node) in scene.nodes.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let depth = scene.ancestors(NodeId(i as u32)).count().saturating_sub(1);
        let _ = write!(
            out,
            "[\"{:?}\",{},{},{},{},{},{},{},{}]",
            node.kind,
            node.parent.map_or(-1, |p| p.0 as i64),
            depth,
            num(node.x),
            num(node.y),
            num(node.width),
            num(node.height),
            num(node.baseline),
            u8::from(editor.is_placeholder(scene, NodeId(i as u32))),
        );
    }
    out.push_str("]}");
    out
}

fn num(v: f32) -> String {
    if v.is_finite() {
        format!("{:.3}", v)
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_owned()
    } else {
        "0".to_owned()
    }
}

fn json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
