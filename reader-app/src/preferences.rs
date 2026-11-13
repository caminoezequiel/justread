//! Reading-comfort preferences and the chapter CSS they produce.

use serde::{Deserialize, Serialize};

pub const MIN_FONT_SCALE: u16 = 60;
pub const MAX_FONT_SCALE: u16 = 200;
const FONT_SCALE_STEP: u16 = 10;
/// Reading pacer speed, in words per minute.
pub const MIN_PACE_WPM: u16 = 80;
pub const MAX_PACE_WPM: u16 = 600;
const PACE_WPM_STEP: u16 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    /// The book's own colors on white.
    Light,
    Sepia,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LineWidth {
    Narrow,
    Medium,
    Wide,
    /// No limit: the text fills the window.
    Full,
}

/// `#[serde(default)]` on the struct fills any field missing from the file
/// (say, one added in a later version) from `Preferences::default()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub theme: Theme,
    /// Text size in percent of the book's own size.
    pub font_scale: u16,
    pub line_width: LineWidth,
    /// Clicking a word (or selecting text) shows its translation.
    pub tap_to_translate: bool,
    /// Speed of the reading pacer (the moving line and word highlight).
    pub pace_wpm: u16,
    /// The pacer also marks the current word, not just its line.
    pub pace_word: bool,
}

impl Default for Preferences {
    fn default() -> Self {
        Preferences {
            theme: Theme::Light,
            font_scale: 100,
            line_width: LineWidth::Medium,
            tap_to_translate: true,
            pace_wpm: 200,
            pace_word: true,
        }
    }
}

impl Theme {
    pub const ALL: [Theme; 3] = [Theme::Light, Theme::Sepia, Theme::Dark];

    pub fn name(self) -> &'static str {
        match self {
            Theme::Light => "light",
            Theme::Sepia => "sepia",
            Theme::Dark => "dark",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Theme::Light => "Light",
            Theme::Sepia => "Sepia",
            Theme::Dark => "Dark",
        }
    }

    /// Colors forced onto the chapter: (background, text, links, code
    /// background). `None` for Light, which keeps the book's own colors.
    /// Reading pacer highlights: (current line, current word). Translucent
    /// so the text stays readable; tinted to match the theme.
    fn pace_colors(self) -> (&'static str, &'static str) {
        match self {
            Theme::Light => ("rgba(255, 213, 79, 0.25)", "rgba(255, 179, 0, 0.5)"),
            Theme::Sepia => ("rgba(156, 74, 18, 0.1)", "rgba(156, 74, 18, 0.28)"),
            Theme::Dark => ("rgba(138, 180, 248, 0.12)", "rgba(138, 180, 248, 0.35)"),
        }
    }

    fn palette(self) -> Option<Palette> {
        match self {
            Theme::Light => None,
            Theme::Sepia => Some(Palette {
                background: "#f4ecd8",
                text: "#5b4636",
                link: "#9c4a12",
                code: "#ebe0c4",
                border: "#d9c9a3",
            }),
            Theme::Dark => Some(Palette {
                background: "#1c1c1e",
                text: "#d6d6d6",
                link: "#8ab4f8",
                code: "#2a2a2d",
                border: "#3a3a3d",
            }),
        }
    }
}

struct Palette {
    background: &'static str,
    text: &'static str,
    link: &'static str,
    code: &'static str,
    border: &'static str,
}

impl LineWidth {
    pub const ALL: [LineWidth; 4] = [
        LineWidth::Narrow,
        LineWidth::Medium,
        LineWidth::Wide,
        LineWidth::Full,
    ];

    pub fn label(self) -> &'static str {
        match self {
            LineWidth::Narrow => "Narrow",
            LineWidth::Medium => "Medium",
            LineWidth::Wide => "Wide",
            LineWidth::Full => "Full",
        }
    }

    /// Maximum width of the text column, in `em` so it follows the font size.
    fn max_width(self) -> Option<&'static str> {
        match self {
            LineWidth::Narrow => Some("32em"),
            LineWidth::Medium => Some("40em"),
            LineWidth::Wide => Some("50em"),
            LineWidth::Full => None,
        }
    }
}

impl Preferences {
    /// One step bigger, up to the maximum. `self` (not `&mut self`): the type
    /// is `Copy`, so methods can take and return values, like `i32::abs`.
    pub fn bigger(self) -> Self {
        let font_scale = (self.font_scale + FONT_SCALE_STEP).min(MAX_FONT_SCALE);
        Preferences { font_scale, ..self }
    }

    pub fn smaller(self) -> Self {
        let font_scale = self
            .font_scale
            .saturating_sub(FONT_SCALE_STEP)
            .max(MIN_FONT_SCALE);
        Preferences { font_scale, ..self }
    }

    /// The pacer one step faster, up to the maximum.
    pub fn faster(self) -> Self {
        let pace_wpm = (self.pace_wpm + PACE_WPM_STEP).min(MAX_PACE_WPM);
        Preferences { pace_wpm, ..self }
    }

    pub fn slower(self) -> Self {
        let pace_wpm = self
            .pace_wpm
            .saturating_sub(PACE_WPM_STEP)
            .max(MIN_PACE_WPM);
        Preferences { pace_wpm, ..self }
    }

    /// Clamp values from a hand-edited or older file into the valid range.
    pub fn sanitized(self) -> Self {
        let font_scale = self.font_scale.clamp(MIN_FONT_SCALE, MAX_FONT_SCALE);
        let pace_wpm = self.pace_wpm.clamp(MIN_PACE_WPM, MAX_PACE_WPM);
        Preferences {
            font_scale,
            pace_wpm,
            ..self
        }
    }

    /// The stylesheet added to every chapter, after the book's own CSS.
    ///
    /// Settings left at their neutral value (Light, 100%, Full) emit nothing,
    /// so the book's design is untouched.
    ///
    /// Books often scope their rules under an id (`#content a {...}`), and
    /// between two `!important` rules the more *specific* one wins, so a plain
    /// `a {...}` would lose. Each `:not(#_jr)` matches every element (no
    /// element has that id) but counts as one id towards specificity:
    /// [`BOOST`] (three ids) beats nearly any book rule, and [`BOOST_MORE`]
    /// lets links and code win over our own catch-all color reset.
    pub fn chapter_css(&self) -> String {
        // Always: room for footnote markers and list numbers that books hang
        // left of the text column (they'd be cut off at the iframe's edge).
        // Padding goes on <html> because books style <body>, rarely <html>.
        let mut css = String::from("html { padding: 0 2.5rem; }\n");
        // The current search match (set by reader.js with the CSS Custom
        // Highlight API).
        css += "::highlight(jr-search) { background-color: #ffd54f; color: #1c1c1e; }\n";
        // The word being translated: translucent, so it works on any theme.
        css += "::highlight(jr-lookup) { background-color: rgba(66, 133, 244, 0.3); }\n";
        // The reading pacer: a soft band on the current line and a stronger
        // mark, underlined like a pencil, on the current word. Without a rule
        // the word's highlight still exists in reader.js but draws nothing.
        let (line, word) = self.theme.pace_colors();
        css += &format!("::highlight(jr-pace-line) {{ background-color: {line}; }}\n");
        if self.pace_word {
            css += &format!(
                "::highlight(jr-pace-word) {{ background-color: {word}; \
                 text-decoration: underline 2px; text-underline-offset: 3px; }}\n"
            );
        }

        if self.font_scale != 100 {
            // Books size text in em/% almost everywhere, so scaling the root
            // font size scales the whole book.
            css += &format!(
                "html{BOOST} {{ font-size: {}% !important; }}\n",
                self.font_scale
            );
        }
        if let Some(width) = self.line_width.max_width() {
            css += &format!(
                "html{BOOST} body {{ max-width: {width} !important; \
                 margin-left: auto !important; margin-right: auto !important; }}\n"
            );
        }
        // PDF pages are canvases, so the theme acts on them directly (see
        // pdf-viewer/viewer.html, whose body has the `jr-pdf` class).
        match self.theme {
            Theme::Light => {}
            Theme::Sepia => css += ".jr-pdf canvas { filter: sepia(0.35); }\n",
            Theme::Dark => css += ".jr-pdf canvas { filter: invert(0.9) hue-rotate(180deg); }\n",
        }
        if let Some(p) = self.theme.palette() {
            css += &format!(
                "html{BOOST}, html{BOOST} body {{ background: {bg} !important; color: {text} !important; }}\n\
                 html{BOOST} body *:not(img):not(svg):not(svg *):not(video) {{ color: inherit !important; \
                 background-color: transparent !important; border-color: {border} !important; }}\n\
                 html{BOOST_MORE} a, html{BOOST_MORE} a * {{ color: {link} !important; }}\n\
                 html{BOOST_MORE} :is(pre, code, kbd, samp, tt) {{ background-color: {code} !important; }}\n",
                bg = p.background,
                text = p.text,
                link = p.link,
                code = p.code,
                border = p.border,
            );
        }
        css
    }
}

/// Specificity boosters for [`Preferences::chapter_css`].
const BOOST: &str = ":not(#_jr):not(#_jr):not(#_jr)";
const BOOST_MORE: &str = ":not(#_jr):not(#_jr):not(#_jr):not(#_jr)";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_keep_the_books_colors_and_size() {
        let css = Preferences::default().chapter_css();
        assert!(css.contains("padding: 0 2.5rem"));
        assert!(css.contains("max-width: 40em"));
        assert!(!css.contains("font-size"));
        assert!(!css.contains("background:"), "no theme colors");
        assert!(css.contains("::highlight(jr-lookup)"));
    }

    #[test]
    fn themes_sizes_and_widths_show_up_in_the_css() {
        let prefs = Preferences {
            theme: Theme::Dark,
            font_scale: 130,
            line_width: LineWidth::Full,
            tap_to_translate: false,
            pace_wpm: 300,
            pace_word: true,
        };
        let css = prefs.chapter_css();
        assert!(css.contains("font-size: 130% !important"));
        assert!(css.contains("background: #1c1c1e !important"));
        assert!(!css.contains("max-width"));
    }

    #[test]
    fn font_scale_stays_in_range() {
        let mut prefs = Preferences::default();
        for _ in 0..50 {
            prefs = prefs.bigger();
        }
        assert_eq!(prefs.font_scale, MAX_FONT_SCALE);
        for _ in 0..50 {
            prefs = prefs.smaller();
        }
        assert_eq!(prefs.font_scale, MIN_FONT_SCALE);
        let odd = Preferences {
            font_scale: 5000,
            ..Preferences::default()
        };
        assert_eq!(odd.sanitized().font_scale, MAX_FONT_SCALE);
    }

    #[test]
    fn pace_stays_in_range() {
        let mut prefs = Preferences::default();
        for _ in 0..200 {
            prefs = prefs.faster();
        }
        assert_eq!(prefs.pace_wpm, MAX_PACE_WPM);
        for _ in 0..200 {
            prefs = prefs.slower();
        }
        assert_eq!(prefs.pace_wpm, MIN_PACE_WPM);
        let odd = Preferences {
            pace_wpm: 0,
            ..Preferences::default()
        };
        assert_eq!(odd.sanitized().pace_wpm, MIN_PACE_WPM);
    }

    #[test]
    fn every_theme_styles_the_pacer() {
        for theme in Theme::ALL {
            let css = Preferences {
                theme,
                ..Preferences::default()
            }
            .chapter_css();
            assert!(css.contains("::highlight(jr-pace-line)"), "{theme:?}");
            assert!(css.contains("::highlight(jr-pace-word)"), "{theme:?}");
        }
    }

    #[test]
    fn pace_word_highlight_can_be_turned_off() {
        let css = Preferences {
            pace_word: false,
            ..Preferences::default()
        }
        .chapter_css();
        assert!(css.contains("::highlight(jr-pace-line)"));
        assert!(!css.contains("jr-pace-word"));
    }

    #[test]
    fn json_is_readable_and_missing_fields_get_defaults() -> Result<(), serde_json::Error> {
        let json = serde_json::to_string(&Preferences {
            theme: Theme::Sepia,
            ..Preferences::default()
        })?;
        assert_eq!(
            json,
            r#"{"theme":"sepia","font_scale":100,"line_width":"medium","tap_to_translate":true,"pace_wpm":200,"pace_word":true}"#
        );
        let partial: Preferences = serde_json::from_str(r#"{"theme":"dark"}"#)?;
        assert_eq!(
            partial,
            Preferences {
                theme: Theme::Dark,
                ..Preferences::default()
            }
        );
        Ok(())
    }
}
