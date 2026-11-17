//! The app's colors. A `ThemePreset` is what the user edits and saves: a
//! background per variant, two accents, a highlight, two warning colors and an
//! optional background image. `EditorTheme` is the full palette derived from
//! a preset for the bright or the dark variant, and `ThemeStore` is the
//! plain-text file that remembers the presets between runs.

use std::path::PathBuf;

use egui::{Color32, Stroke};

/// The user's light/dark preference. `System` follows the OS and falls back
/// to dark when it does not say.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppThemeMode {
    System,
    Dark,
    Bright,
}

impl AppThemeMode {
    pub const ALL: [AppThemeMode; 3] = [
        AppThemeMode::System,
        AppThemeMode::Dark,
        AppThemeMode::Bright,
    ];

    pub fn label(self) -> &'static str {
        match self {
            AppThemeMode::System => "System",
            AppThemeMode::Dark => "Dark",
            AppThemeMode::Bright => "Bright",
        }
    }

    fn key(self) -> &'static str {
        match self {
            AppThemeMode::System => "system",
            AppThemeMode::Dark => "dark",
            AppThemeMode::Bright => "bright",
        }
    }

    fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|mode| mode.key() == key)
    }
}

/// The parts of the interface whose color can be set on its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeElement {
    /// Selected rows and toggled buttons: the track list, toolbar toggles.
    Selection,
    Cursor,
    PlaybackLine,
    /// Bar blocks of the bar timeline.
    Timeline,
    /// The active tool in the left palette.
    Sidebar,
    Fretboard,
    SelectedNotes,
    TempoAutomation,
}

impl ThemeElement {
    pub const ALL: [ThemeElement; 8] = [
        ThemeElement::Selection,
        ThemeElement::Cursor,
        ThemeElement::PlaybackLine,
        ThemeElement::Timeline,
        ThemeElement::Sidebar,
        ThemeElement::Fretboard,
        ThemeElement::SelectedNotes,
        ThemeElement::TempoAutomation,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ThemeElement::Selection => "Selection and toggles",
            ThemeElement::Cursor => "Edit cursor",
            ThemeElement::PlaybackLine => "Playback line",
            ThemeElement::Timeline => "Bar timeline",
            ThemeElement::Sidebar => "Left sidebar",
            ThemeElement::Fretboard => "Fretboard",
            ThemeElement::SelectedNotes => "Selected notes",
            ThemeElement::TempoAutomation => "Tempo automation",
        }
    }

    fn key(self) -> &'static str {
        match self {
            ThemeElement::Selection => "selection",
            ThemeElement::Cursor => "cursor",
            ThemeElement::PlaybackLine => "playback_line",
            ThemeElement::Timeline => "timeline",
            ThemeElement::Sidebar => "sidebar",
            ThemeElement::Fretboard => "fretboard",
            ThemeElement::SelectedNotes => "selected_notes",
            ThemeElement::TempoAutomation => "tempo_automation",
        }
    }

    fn index(self) -> usize {
        Self::ALL.iter().position(|element| *element == self).unwrap_or(0)
    }
}

/// Where an element gets its color from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorSource {
    Accent,
    Accent2,
    Custom(Color32),
}

#[derive(Clone, Debug, PartialEq)]
pub struct ThemePreset {
    pub name: String,
    pub bright_background: Color32,
    pub dark_background: Color32,
    pub accent: Color32,
    pub accent2: Color32,
    /// Bar blocks of the timeline while they are being played.
    pub highlight: Color32,
    pub warning: Color32,
    /// A second, red warning: a muted track and bypassed effects.
    pub warning2: Color32,
    pub background_image: Option<PathBuf>,
    /// How much of the background image shows through the sheet, 0 to 1.
    pub image_visibility: f32,
    elements: [ColorSource; ThemeElement::ALL.len()],
}

impl ThemePreset {
    pub fn source(&self, element: ThemeElement) -> ColorSource {
        self.elements[element.index()]
    }

    pub fn set_source(&mut self, element: ThemeElement, source: ColorSource) {
        self.elements[element.index()] = source;
    }

    pub fn color_of(&self, element: ThemeElement) -> Color32 {
        match self.source(element) {
            ColorSource::Accent => self.accent,
            ColorSource::Accent2 => self.accent2,
            ColorSource::Custom(color) => color,
        }
    }

    fn colors(
        name: &str,
        bright_background: [u8; 3],
        dark_background: [u8; 3],
        accent: [u8; 3],
        accent2: [u8; 3],
        highlight: [u8; 3],
        warning: [u8; 3],
    ) -> Self {
        let rgb = |[r, g, b]: [u8; 3]| Color32::from_rgb(r, g, b);
        Self {
            name: name.to_owned(),
            bright_background: rgb(bright_background),
            dark_background: rgb(dark_background),
            accent: rgb(accent),
            accent2: rgb(accent2),
            highlight: rgb(highlight),
            warning: rgb(warning),
            warning2: rgb(DEFAULT_WARNING2),
            background_image: None,
            image_visibility: 0.3,
            elements: [ColorSource::Accent; ThemeElement::ALL.len()],
        }
    }

    /// The look the app has by default: a blue first accent about as light
    /// as the green second one.
    pub fn standard() -> Self {
        Self::colors(
            "BetterWriter",
            [247, 247, 247],
            [20, 21, 23],
            STANDARD_ACCENT,
            [64, 196, 184],
            [255, 176, 59],
            [250, 204, 21],
        )
    }
}

/// The default Warning 2, a red.
const DEFAULT_WARNING2: [u8; 3] = [226, 72, 72];

/// The default Accent 1, a light blue.
const STANDARD_ACCENT: [u8; 3] = [76, 164, 240];
/// The darker blue the default had before; a saved copy of the standard look
/// that still has it is moved to the current default when it is read.
const OLD_STANDARD_ACCENT: [u8; 3] = [0, 92, 128];

/// The presets that ship with the app. They cannot be changed or deleted;
/// edit a copy under another name.
pub fn built_in_presets() -> Vec<ThemePreset> {
    vec![
        ThemePreset::standard(),
        ThemePreset::colors(
            "Nordic Night",
            [236, 239, 244],
            [46, 52, 64],
            [94, 129, 172],
            [136, 192, 208],
            [163, 190, 140],
            [235, 203, 139],
        ),
        ThemePreset::colors(
            "Sunset Jazz",
            [253, 246, 238],
            [38, 28, 34],
            [224, 98, 78],
            [240, 166, 60],
            [255, 214, 102],
            [250, 204, 21],
        ),
        ThemePreset::colors(
            "Forest Floor",
            [238, 244, 237],
            [22, 32, 27],
            [46, 139, 87],
            [156, 204, 101],
            [255, 202, 40],
            [255, 193, 7],
        ),
        ThemePreset::colors(
            "Ocean Deep",
            [236, 245, 250],
            [13, 27, 42],
            [14, 124, 178],
            [38, 198, 218],
            [255, 183, 77],
            [253, 216, 53],
        ),
        ThemePreset::colors(
            "Rose Quartz",
            [251, 241, 245],
            [36, 27, 35],
            [194, 70, 132],
            [142, 110, 214],
            [255, 171, 145],
            [250, 204, 21],
        ),
        ThemePreset::colors(
            "Graphite and Gold",
            [244, 242, 237],
            [29, 29, 31],
            [184, 140, 28],
            [120, 130, 145],
            [240, 200, 90],
            [250, 204, 21],
        ),
        ThemePreset::colors(
            "Midnight Violet",
            [240, 238, 250],
            [24, 22, 40],
            [112, 84, 214],
            [86, 204, 242],
            [255, 121, 198],
            [250, 204, 21],
        ),
    ]
}

#[derive(Clone, Copy, Debug)]
pub struct EditorTheme {
    pub sheet_background: Color32,
    pub panel_background: Color32,
    pub menu_bar_background: Color32,
    pub palette_background: Color32,
    pub fretboard_background: Color32,
    pub fretboard_wood: Color32,
    pub fretboard_wire: Color32,
    pub fretboard_nut: Color32,
    pub fretboard_inlay: Color32,
    pub fretboard_string: Color32,
    /// Glyph color of palette icons and the fill of the hovered one.
    pub palette_icon: Color32,
    pub palette_hover: Color32,
    pub palette_separator: Color32,
    pub track_table_background: Color32,
    /// The area behind the "page" (the sheet itself is `sheet_background`).
    pub canvas_backdrop: Color32,
    pub notation_foreground: Color32,
    pub muted_foreground: Color32,
    pub staff_line_color: Color32,
    pub bar_line_color: Color32,
    pub rest_color: Color32,
    pub note_fill: Color32,
    pub note_stroke: Color32,
    pub accent: Color32,
    pub accent2: Color32,
    pub highlight_color: Color32,
    pub warning_color: Color32,
    pub warning2_color: Color32,
    pub selection_color: Color32,
    pub cursor_color: Color32,
    pub playback_color: Color32,
    pub timeline_color: Color32,
    pub sidebar_color: Color32,
    pub fretboard_color: Color32,
    pub note_selection_color: Color32,
    pub automation_color: Color32,
    /// Whether the surfaces are dark, which decides the text colors.
    pub dark: bool,
}

fn mix(from: Color32, to: Color32, amount: f32) -> Color32 {
    let channel = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * amount).round() as u8;
    Color32::from_rgb(
        channel(from.r(), to.r()),
        channel(from.g(), to.g()),
        channel(from.b(), to.b()),
    )
}

/// Perceived brightness from 0 (black) to 1 (white).
pub fn luminance(color: Color32) -> f32 {
    (0.2126 * color.r() as f32 + 0.7152 * color.g() as f32 + 0.0722 * color.b() as f32) / 255.0
}

/// How much of the selection color shows in selected rows and toggled
/// buttons; the rest is the panel behind them. The same weight as a selected
/// bar in the bar timeline, so the two read as one color.
const SELECTION_STRENGTH: f32 = 0.5;

/// Black or white text, whichever reads better on `background`.
pub fn contrast_text(background: Color32) -> Color32 {
    if luminance(background) > 0.55 {
        Color32::from_rgb(20, 22, 24)
    } else {
        Color32::WHITE
    }
}

impl EditorTheme {
    /// The palette for `preset` on its bright or its dark background. The
    /// text, lines and wood follow how light the background really is, so a
    /// dark color in the bright slot still reads.
    pub fn from_preset(preset: &ThemePreset, bright: bool) -> Self {
        let background = if bright {
            preset.bright_background
        } else {
            preset.dark_background
        };
        let dark = luminance(background) < 0.5;
        let (white, black) = (Color32::WHITE, Color32::BLACK);
        // Chrome moves away from the background: lighter on dark surfaces,
        // darker on light ones.
        let step = |dark_amount: f32, light_amount: f32| {
            if dark {
                mix(background, white, dark_amount)
            } else {
                mix(background, black, light_amount)
            }
        };
        let pick = |on_dark: [u8; 3], on_light: [u8; 3]| {
            let [r, g, b] = if dark { on_dark } else { on_light };
            Color32::from_rgb(r, g, b)
        };
        Self {
            canvas_backdrop: background,
            sheet_background: if dark {
                mix(background, white, 0.07)
            } else {
                mix(background, Color32::from_rgb(238, 236, 226), 0.45)
            },
            panel_background: step(0.02, 0.06),
            menu_bar_background: step(0.05, 0.035),
            palette_background: step(0.06, 0.05),
            track_table_background: step(0.05, 0.03),
            fretboard_background: mix(background, black, if dark { 0.6 } else { 0.96 }),
            fretboard_wood: pick([52, 34, 26], [70, 45, 33]),
            fretboard_wire: pick([160, 163, 168], [188, 190, 194]),
            fretboard_nut: pick([210, 204, 186], [232, 226, 208]),
            fretboard_inlay: pick([178, 172, 156], [214, 207, 188]),
            fretboard_string: pick([190, 190, 185], [208, 208, 202]),
            palette_icon: pick([206, 209, 212], [50, 55, 60]),
            palette_hover: step(0.12, 0.10),
            palette_separator: step(0.10, 0.09),
            notation_foreground: pick([226, 227, 224], [30, 32, 34]),
            muted_foreground: pick([150, 155, 158], [104, 111, 116]),
            staff_line_color: pick([92, 96, 94], [170, 174, 171]),
            bar_line_color: pick([140, 145, 148], [95, 102, 105]),
            rest_color: pick([158, 161, 164], [98, 101, 104]),
            note_fill: pick([52, 55, 59], [255, 255, 252]),
            note_stroke: pick([214, 216, 219], [33, 37, 41]),
            accent: preset.accent,
            accent2: preset.accent2,
            highlight_color: preset.highlight,
            warning_color: preset.warning,
            warning2_color: preset.warning2,
            selection_color: preset.color_of(ThemeElement::Selection),
            cursor_color: preset.color_of(ThemeElement::Cursor),
            playback_color: preset.color_of(ThemeElement::PlaybackLine),
            timeline_color: preset.color_of(ThemeElement::Timeline),
            sidebar_color: preset.color_of(ThemeElement::Sidebar),
            fretboard_color: preset.color_of(ThemeElement::Fretboard),
            note_selection_color: preset.color_of(ThemeElement::SelectedNotes),
            automation_color: preset.color_of(ThemeElement::TempoAutomation),
            dark,
        }
    }

    /// egui's own widgets (buttons, menus, text fields, windows) in this
    /// palette, so they match what is painted by hand.
    pub fn visuals(&self) -> egui::Visuals {
        let mut visuals = if self.dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        };
        let pole = if self.dark { Color32::WHITE } else { Color32::BLACK };
        let surface = self.canvas_backdrop;
        let text = self.notation_foreground;
        visuals.panel_fill = self.panel_background;
        visuals.window_fill = self.menu_bar_background;
        visuals.window_stroke = Stroke::new(1.0_f32, self.palette_separator);
        visuals.extreme_bg_color = if self.dark {
            mix(surface, Color32::BLACK, 0.35)
        } else {
            mix(surface, Color32::WHITE, 0.6)
        };
        visuals.faint_bg_color = mix(surface, pole, 0.03);
        visuals.hyperlink_color = self.selection_color;
        visuals.warn_fg_color = self.warning_color;
        let selection_fill = mix(self.panel_background, self.selection_color, SELECTION_STRENGTH);
        visuals.selection.bg_fill = selection_fill;
        visuals.selection.stroke = Stroke::new(1.0_f32, contrast_text(selection_fill));

        let widgets = &mut visuals.widgets;
        widgets.noninteractive.bg_fill = self.panel_background;
        widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, self.palette_separator);
        widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, text);
        for (state, amount) in [
            (&mut widgets.inactive, 0.10),
            (&mut widgets.hovered, 0.17),
            (&mut widgets.active, 0.24),
            (&mut widgets.open, 0.17),
        ] {
            let fill = mix(surface, pole, amount * if self.dark { 1.0 } else { 0.9 });
            state.bg_fill = fill;
            state.weak_bg_fill = fill;
            state.bg_stroke = Stroke::new(1.0_f32, mix(fill, pole, 0.12));
            state.fg_stroke = Stroke::new(1.0_f32, text);
        }
        visuals
    }

    pub fn staff_stroke(self) -> Stroke {
        Stroke::new(1.0_f32, self.staff_line_color)
    }

    pub fn bar_stroke(self) -> Stroke {
        Stroke::new(1.5_f32, self.bar_line_color)
    }

    pub fn note_stroke(self) -> Stroke {
        Stroke::new(1.25_f32, self.note_stroke)
    }
}

impl Default for EditorTheme {
    fn default() -> Self {
        Self::from_preset(&ThemePreset::standard(), false)
    }
}

fn to_hex(color: Color32) -> String {
    format!("#{:02x}{:02x}{:02x}", color.r(), color.g(), color.b())
}

fn from_hex(text: &str) -> Option<Color32> {
    let digits = text.trim().strip_prefix('#')?;
    if digits.len() != 6 {
        return None;
    }
    let value = u32::from_str_radix(digits, 16).ok()?;
    Some(Color32::from_rgb((value >> 16) as u8, (value >> 8) as u8, value as u8))
}

impl ThemePreset {
    fn write_block(&self, header: &str, out: &mut String) {
        out.push_str(&format!("[{header}]\n"));
        out.push_str(&format!("name={}\n", self.name.replace(['\n', '\r'], " ")));
        for (key, color) in [
            ("bright_background", self.bright_background),
            ("dark_background", self.dark_background),
            ("accent", self.accent),
            ("accent2", self.accent2),
            ("highlight", self.highlight),
            ("warning", self.warning),
            ("warning2", self.warning2),
        ] {
            out.push_str(&format!("{key}={}\n", to_hex(color)));
        }
        if let Some(path) = &self.background_image {
            out.push_str(&format!("image={}\n", path.display()));
        }
        out.push_str(&format!("image_visibility={}\n", self.image_visibility));
        for element in ThemeElement::ALL {
            let source = match self.source(element) {
                ColorSource::Accent => "accent".to_owned(),
                ColorSource::Accent2 => "accent2".to_owned(),
                ColorSource::Custom(color) => to_hex(color),
            };
            out.push_str(&format!("element.{}={source}\n", element.key()));
        }
    }

    fn read_block(lines: &[&str]) -> Self {
        let mut preset = Self::standard();
        preset.name = "Custom".to_owned();
        for line in lines {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim();
            let color = from_hex(value);
            match key.trim() {
                "name" if !value.is_empty() => preset.name = value.to_owned(),
                "bright_background" => preset.bright_background = color.unwrap_or(preset.bright_background),
                "dark_background" => preset.dark_background = color.unwrap_or(preset.dark_background),
                "accent" => preset.accent = color.unwrap_or(preset.accent),
                "accent2" => preset.accent2 = color.unwrap_or(preset.accent2),
                "highlight" => preset.highlight = color.unwrap_or(preset.highlight),
                "warning" => preset.warning = color.unwrap_or(preset.warning),
                "warning2" => preset.warning2 = color.unwrap_or(preset.warning2),
                "image" if !value.is_empty() => preset.background_image = Some(PathBuf::from(value)),
                "image_visibility" => {
                    preset.image_visibility = value.parse::<f32>().map_or(0.3, |v| v.clamp(0.0, 1.0));
                }
                key => {
                    let Some(element) = key
                        .strip_prefix("element.")
                        .and_then(|name| ThemeElement::ALL.into_iter().find(|e| e.key() == name))
                    else {
                        continue;
                    };
                    let source = match value {
                        "accent" => Some(ColorSource::Accent),
                        "accent2" => Some(ColorSource::Accent2),
                        other => from_hex(other).map(ColorSource::Custom),
                    };
                    if let Some(source) = source {
                        preset.set_source(element, source);
                    }
                }
            }
        }
        preset
    }
}

/// Everything remembered between runs: the light/dark choice, the theme
/// being edited (saved or not) and the presets the user saved.
#[derive(Clone, Debug, PartialEq)]
pub struct ThemeStore {
    pub mode: AppThemeMode,
    pub current: ThemePreset,
    pub presets: Vec<ThemePreset>,
}

impl Default for ThemeStore {
    fn default() -> Self {
        Self {
            mode: AppThemeMode::System,
            current: ThemePreset::standard(),
            presets: Vec::new(),
        }
    }
}

impl ThemeStore {
    pub fn to_text(&self) -> String {
        let mut out = format!("mode={}\n", self.mode.key());
        self.current.write_block("current", &mut out);
        for preset in &self.presets {
            preset.write_block("preset", &mut out);
        }
        out
    }

    /// Reads a store back. Anything missing or unreadable falls back to the
    /// standard look rather than failing.
    pub fn from_text(text: &str) -> Self {
        let mut store = Self::default();
        let mut blocks: Vec<(&str, Vec<&str>)> = Vec::new();
        for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
            if let Some(header) = line.strip_prefix('[').and_then(|rest| rest.strip_suffix(']')) {
                blocks.push((header, Vec::new()));
            } else if let Some((_, lines)) = blocks.last_mut() {
                lines.push(line);
            } else if let Some(mode) = line.strip_prefix("mode=").and_then(AppThemeMode::from_key) {
                store.mode = mode;
            }
        }
        let built_in = built_in_presets();
        for (header, lines) in blocks {
            let mut preset = ThemePreset::read_block(&lines);
            let [r, g, b] = OLD_STANDARD_ACCENT;
            if preset.name == ThemePreset::standard().name && preset.accent == Color32::from_rgb(r, g, b) {
                preset.accent = ThemePreset::standard().accent;
            }
            match header {
                "current" => store.current = preset,
                "preset" => {
                    let taken = built_in
                        .iter()
                        .chain(&store.presets)
                        .any(|other| other.name.eq_ignore_ascii_case(&preset.name));
                    if !taken {
                        store.presets.push(preset);
                    }
                }
                _ => {}
            }
        }
        store
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_colors_round_trip() {
        let color = Color32::from_rgb(0, 92, 128);
        assert_eq!(to_hex(color), "#005c80");
        assert_eq!(from_hex("#005c80"), Some(color));
        assert_eq!(from_hex("005c80"), None);
        assert_eq!(from_hex("#12345"), None);
        assert_eq!(from_hex("#gggggg"), None);
    }

    #[test]
    fn the_standard_look_is_a_blue_accent_with_the_green_second() {
        let preset = ThemePreset::standard();
        assert_eq!(preset.accent, Color32::from_rgb(76, 164, 240));
        // Light enough to stand next to the green second accent.
        assert!((luminance(preset.accent) - luminance(preset.accent2)).abs() < 0.12);
        assert_eq!(preset.accent2, Color32::from_rgb(64, 196, 184));
        for element in ThemeElement::ALL {
            assert_eq!(preset.color_of(element), preset.accent);
        }
    }

    #[test]
    fn an_element_can_follow_either_accent_or_its_own_color() {
        let mut preset = ThemePreset::standard();
        preset.set_source(ThemeElement::Cursor, ColorSource::Accent2);
        preset.set_source(ThemeElement::Fretboard, ColorSource::Custom(Color32::RED));
        let theme = EditorTheme::from_preset(&preset, false);
        assert_eq!(theme.cursor_color, preset.accent2);
        assert_eq!(theme.fretboard_color, Color32::RED);
        assert_eq!(theme.timeline_color, preset.accent);
    }

    #[test]
    fn built_in_names_are_unique_and_every_preset_has_readable_text() {
        let presets = built_in_presets();
        for (index, preset) in presets.iter().enumerate() {
            assert!(
                presets[index + 1..]
                    .iter()
                    .all(|other| !other.name.eq_ignore_ascii_case(&preset.name)),
                "{}",
                preset.name
            );
            for bright in [true, false] {
                let theme = EditorTheme::from_preset(preset, bright);
                let gap = (luminance(theme.notation_foreground) - luminance(theme.sheet_background)).abs();
                assert!(gap > 0.5, "{} bright={bright}: {gap}", preset.name);
            }
        }
    }

    #[test]
    fn the_variant_picks_its_own_background_and_text() {
        let preset = ThemePreset::standard();
        let bright = EditorTheme::from_preset(&preset, true);
        let dark = EditorTheme::from_preset(&preset, false);
        assert_eq!(bright.canvas_backdrop, preset.bright_background);
        assert_eq!(dark.canvas_backdrop, preset.dark_background);
        assert!(!bright.dark && dark.dark);
        assert!(luminance(bright.notation_foreground) < luminance(dark.notation_foreground));
    }

    #[test]
    fn a_dark_color_in_the_bright_slot_still_gets_light_text() {
        let mut preset = ThemePreset::standard();
        preset.bright_background = Color32::from_rgb(10, 10, 30);
        let theme = EditorTheme::from_preset(&preset, true);
        assert!(theme.dark);
        assert!(luminance(theme.notation_foreground) > 0.7);
    }

    #[test]
    fn selected_rows_and_toggles_are_a_softer_mix_of_the_accent() {
        let preset = ThemePreset::standard();
        let theme = EditorTheme::from_preset(&preset, false);
        let fill = theme.visuals().selection.bg_fill;
        assert_eq!(fill, mix(theme.panel_background, preset.accent, SELECTION_STRENGTH));
        assert!(luminance(fill) < luminance(preset.accent));
        let text = theme.visuals().selection.stroke.color;
        assert!((luminance(text) - luminance(fill)).abs() > 0.4);
    }

    #[test]
    fn text_on_an_accent_contrasts_with_it() {
        assert_eq!(contrast_text(Color32::from_rgb(0, 92, 128)), Color32::WHITE);
        assert_ne!(contrast_text(Color32::from_rgb(76, 164, 240)), Color32::WHITE);
        assert_ne!(contrast_text(Color32::from_rgb(250, 230, 120)), Color32::WHITE);
    }

    #[test]
    fn the_store_round_trips_through_text() {
        let mut custom = ThemePreset::standard();
        custom.name = "Mine".to_owned();
        custom.accent = Color32::from_rgb(1, 2, 3);
        custom.warning2 = Color32::from_rgb(200, 10, 20);
        custom.background_image = Some(PathBuf::from("/tmp/a b.png"));
        custom.image_visibility = 0.55;
        custom.set_source(ThemeElement::Sidebar, ColorSource::Accent2);
        custom.set_source(ThemeElement::Cursor, ColorSource::Custom(Color32::from_rgb(9, 8, 7)));
        let store = ThemeStore {
            mode: AppThemeMode::Bright,
            current: custom.clone(),
            presets: vec![custom],
        };
        assert_eq!(ThemeStore::from_text(&store.to_text()), store);
    }

    #[test]
    fn warning_2_is_red_and_a_theme_file_without_it_gets_the_default() {
        let preset = ThemePreset::standard();
        let [r, g, b, _] = preset.warning2.to_array();
        assert!(r > 180 && g < 100 && b < 100);
        let old = ThemePreset::read_block(&["name=Old", "warning=#facc15"]);
        assert_eq!(old.warning2, preset.warning2);
    }

    #[test]
    fn a_saved_copy_of_the_old_default_accent_moves_to_the_new_one() {
        let old = "mode=dark\n[current]\nname=BetterWriter\naccent=#005c80\n";
        assert_eq!(ThemeStore::from_text(old).current.accent, ThemePreset::standard().accent);
        let custom = "[current]\nname=Mine\naccent=#005c80\n";
        assert_eq!(ThemeStore::from_text(custom).current.accent, Color32::from_rgb(0, 92, 128));
    }

    #[test]
    fn a_broken_file_falls_back_to_the_standard_look() {
        assert_eq!(ThemeStore::from_text(""), ThemeStore::default());
        let store = ThemeStore::from_text("mode=sideways\n[current]\naccent=nope\nimage_visibility=x\n");
        assert_eq!(store.mode, AppThemeMode::System);
        assert_eq!(store.current.accent, ThemePreset::standard().accent);
    }

    #[test]
    fn saved_presets_cannot_take_a_built_in_name() {
        let mut clash = ThemePreset::standard();
        clash.name = "nordic night".to_owned();
        let store = ThemeStore {
            mode: AppThemeMode::Dark,
            current: ThemePreset::standard(),
            presets: vec![clash],
        };
        assert!(ThemeStore::from_text(&store.to_text()).presets.is_empty());
    }
}
