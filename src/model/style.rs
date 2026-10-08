//! TeX's four maths styles and the transitions between them.

use crate::font::MathFont;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MathStyle {
    Display,
    DisplayCramped,
    Text,
    TextCramped,
    Script,
    ScriptCramped,
    ScriptScript,
    ScriptScriptCramped,
}

/// TeX's fallbacks, used when a font leaves a scale percentage at zero.
const DEFAULT_SCRIPT_PERCENT: i16 = 70;
const DEFAULT_SCRIPT_SCRIPT_PERCENT: i16 = 50;

impl MathStyle {
    /// 0 for display and text, 1 for script, 2 for scriptscript.
    #[must_use]
    pub fn size_level(self) -> u8 {
        match self {
            MathStyle::Display
            | MathStyle::DisplayCramped
            | MathStyle::Text
            | MathStyle::TextCramped => 0,
            MathStyle::Script | MathStyle::ScriptCramped => 1,
            MathStyle::ScriptScript | MathStyle::ScriptScriptCramped => 2,
        }
    }

    #[must_use]
    pub fn is_display(self) -> bool {
        matches!(self, MathStyle::Display | MathStyle::DisplayCramped)
    }

    #[must_use]
    pub fn is_cramped(self) -> bool {
        matches!(
            self,
            MathStyle::DisplayCramped
                | MathStyle::TextCramped
                | MathStyle::ScriptCramped
                | MathStyle::ScriptScriptCramped
        )
    }

    /// Whether the suppressed entries of the spacing table apply.
    #[must_use]
    pub fn is_script(self) -> bool {
        self.size_level() > 0
    }

    #[must_use]
    pub fn cramped(self) -> MathStyle {
        match self {
            MathStyle::Display | MathStyle::DisplayCramped => MathStyle::DisplayCramped,
            MathStyle::Text | MathStyle::TextCramped => MathStyle::TextCramped,
            MathStyle::Script | MathStyle::ScriptCramped => MathStyle::ScriptCramped,
            MathStyle::ScriptScript | MathStyle::ScriptScriptCramped => {
                MathStyle::ScriptScriptCramped
            }
        }
    }

    #[must_use]
    pub fn uncramped(self) -> MathStyle {
        match self {
            MathStyle::Display | MathStyle::DisplayCramped => MathStyle::Display,
            MathStyle::Text | MathStyle::TextCramped => MathStyle::Text,
            MathStyle::Script | MathStyle::ScriptCramped => MathStyle::Script,
            MathStyle::ScriptScript | MathStyle::ScriptScriptCramped => MathStyle::ScriptScript,
        }
    }

    /// One size step down: D, T → S; S → SS; SS stays. Cramping is preserved.
    #[must_use]
    fn smaller(self) -> MathStyle {
        let smaller = match self.size_level() {
            0 => MathStyle::Script,
            _ => MathStyle::ScriptScript,
        };
        if self.is_cramped() {
            smaller.cramped()
        } else {
            smaller
        }
    }

    #[must_use]
    pub fn superscript(self) -> MathStyle {
        self.smaller()
    }

    /// A superscript's size, always cramped.
    #[must_use]
    pub fn subscript(self) -> MathStyle {
        self.smaller().cramped()
    }

    /// Display drops to text; everything else takes a size step. Cramping is preserved.
    #[must_use]
    pub fn numerator(self) -> MathStyle {
        match self {
            MathStyle::Display => MathStyle::Text,
            MathStyle::DisplayCramped => MathStyle::TextCramped,
            _ => self.smaller(),
        }
    }

    /// The numerator's style, cramped.
    #[must_use]
    pub fn denominator(self) -> MathStyle {
        self.numerator().cramped()
    }

    /// The multiplier this style applies to the outermost font size.
    #[must_use]
    pub fn scale(self, font: &MathFont<'_>) -> f32 {
        match self.size_level() {
            0 => 1.0,
            1 => font.script_scale(),
            _ => font.script_script_scale(),
        }
    }
}

/// The two script multipliers, read once at font construction.
pub(crate) fn script_scales(constants: &ttf_parser::math::Constants<'_>) -> (f32, f32) {
    let scale = |percent: i16, fallback: i16| {
        f32::from(if percent > 0 { percent } else { fallback }) / 100.0
    };
    (
        scale(
            constants.script_percent_scale_down(),
            DEFAULT_SCRIPT_PERCENT,
        ),
        scale(
            constants.script_script_percent_scale_down(),
            DEFAULT_SCRIPT_SCRIPT_PERCENT,
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_sizes_bottom_out() {
        assert_eq!(MathStyle::Display.superscript(), MathStyle::Script);
        assert_eq!(MathStyle::Text.superscript(), MathStyle::Script);
        assert_eq!(MathStyle::Script.superscript(), MathStyle::ScriptScript);
        assert_eq!(
            MathStyle::ScriptScript.superscript(),
            MathStyle::ScriptScript
        );
    }

    #[test]
    fn subscripts_are_always_cramped() {
        for style in [
            MathStyle::Display,
            MathStyle::Text,
            MathStyle::Script,
            MathStyle::ScriptScript,
        ] {
            assert!(style.subscript().is_cramped(), "{style:?}");
        }
    }

    #[test]
    fn cramping_propagates_through_scripts() {
        assert!(MathStyle::DisplayCramped.superscript().is_cramped());
        assert!(MathStyle::TextCramped.numerator().is_cramped());
    }

    #[test]
    fn a_displayed_numerator_is_text_size() {
        assert_eq!(MathStyle::Display.numerator(), MathStyle::Text);
        assert_eq!(MathStyle::Display.denominator(), MathStyle::TextCramped);
        assert_eq!(MathStyle::Text.numerator(), MathStyle::Script);
    }

    #[test]
    fn only_display_is_display() {
        assert!(MathStyle::Display.is_display());
        assert!(MathStyle::DisplayCramped.is_display());
        assert!(!MathStyle::Text.is_display());
    }
}
