//! eeltex for the browser: a C-ABI module for `wasm32-unknown-unknown`.
//!
//! See `README.md` for the exports and the JavaScript side. There is one input buffer and one
//! output buffer, both owned here, so the host never frees anything.

#![allow(unsafe_code)] // the ABI hands out raw pointers into our own buffers
#![allow(clippy::missing_safety_doc)]

use std::cell::RefCell;

use eeltex::export::svg::{SpriteOptions, SvgOptions, SvgSprite, to_svg};
use eeltex::{Fonts, MathStyle, Options};

/// Separates formulas in and fragments out of `batch`.
const RS: char = '\x1e';
/// Separates the fragments from the trailing `<defs>` in `batch`'s output.
const GS: char = '\x1d';
/// Marks a fragment that is an error message instead of an `<svg>`.
const US: char = '\x1f';

struct State {
    fonts: Fonts<'static>,
    font_px: f32,
    sprite: SvgSprite,
}

thread_local! {
    static INPUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    static OUTPUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

fn set_output(s: &str) {
    OUTPUT.with(|o| {
        let mut o = o.borrow_mut();
        o.clear();
        o.extend_from_slice(s.as_bytes());
    });
}

fn input(len: usize) -> String {
    INPUT.with(|i| {
        let i = i.borrow();
        String::from_utf8_lossy(i.get(..len.min(i.len())).unwrap_or(&[])).into_owned()
    })
}

/// A buffer of `len` bytes for the next call's input.
#[unsafe(no_mangle)]
pub extern "C" fn in_ptr(len: usize) -> *mut u8 {
    INPUT.with(|i| {
        let mut i = i.borrow_mut();
        i.clear();
        i.resize(len, 0);
        i.as_mut_ptr()
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn out_ptr() -> *const u8 {
    OUTPUT.with(|o| o.borrow().as_ptr())
}

#[unsafe(no_mangle)]
pub extern "C" fn out_len() -> usize {
    OUTPUT.with(|o| o.borrow().len())
}

/// Load the embedded font at `font_px` pixels per em. Returns 1, or 0 with the error as output.
#[unsafe(no_mangle)]
pub extern "C" fn init(font_px: f32) -> u32 {
    match Fonts::embedded() {
        Ok(fonts) => {
            STATE.with(|s| {
                *s.borrow_mut() = Some(State {
                    fonts,
                    font_px: if font_px.is_finite() && font_px > 0.0 {
                        font_px
                    } else {
                        16.0
                    },
                    sprite: SvgSprite::new(SpriteOptions::default()),
                });
            });
            set_output("");
            1
        }
        Err(e) => {
            set_output(&e.to_string());
            0
        }
    }
}

fn options(state: &State, display: u32) -> Options {
    Options::default()
        .with_font_size(state.font_px)
        .with_style(if display == 0 {
            MathStyle::Text
        } else {
            MathStyle::Display
        })
}

/// One formula (the first `len` bytes of the input) as a standalone SVG document.
#[unsafe(no_mangle)]
pub extern "C" fn render(len: usize, display: u32) -> u32 {
    let src = input(len);
    STATE.with(|s| {
        let s = s.borrow();
        let Some(state) = s.as_ref() else {
            set_output("init() first");
            return 0;
        };
        let result = eeltex::render(&src, &state.fonts, &options(state, display))
            .and_then(|scene| to_svg(&scene, &state.fonts, &SvgOptions::default()));
        match result {
            Ok(svg) => {
                set_output(&svg);
                1
            }
            Err(e) => {
                set_output(&e.to_string());
                0
            }
        }
    })
}

/// Formulas separated by `\x1e` → fragments separated by `\x1e`, then `\x1d` and the new defs.
///
/// A fragment is `<svg>…</svg>`, or `\x1f` followed by the parse or layout error. Returns the
/// number of fragments.
#[unsafe(no_mangle)]
pub extern "C" fn batch(len: usize, display: u32) -> u32 {
    let src = input(len);
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        let Some(state) = s.as_mut() else {
            set_output("init() first");
            return 0;
        };
        let opts = options(state, display);
        let mut out = String::with_capacity(src.len() * 8);
        let mut n = 0u32;
        for (k, formula) in src.split(RS).enumerate() {
            if k > 0 {
                out.push(RS);
            }
            n += 1;
            let result = eeltex::render(formula, &state.fonts, &opts)
                .and_then(|scene| state.sprite.fragment(&scene, &state.fonts));
            match result {
                Ok(fragment) => out.push_str(&fragment),
                Err(e) => {
                    out.push(US);
                    out.push_str(&e.to_string());
                }
            }
        }
        out.push(GS);
        out.push_str(&state.sprite.take_defs());
        set_output(&out);
        n
    })
}

/// Forget the sprite's glyph table; the next `batch` re-sends every outline it uses.
#[unsafe(no_mangle)]
pub extern "C" fn reset() {
    STATE.with(|s| {
        if let Some(state) = s.borrow_mut().as_mut() {
            state.sprite = SvgSprite::new(SpriteOptions::default());
        }
    });
}
