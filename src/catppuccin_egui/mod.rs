//! Soothing pastel theme for egui.
//!
//! Vendored from <https://github.com/catppuccin/egui> (`src/lib.rs`), which has not
//! been updated in ~8 months and only supports egui up to 0.33. This copy is trimmed
//! to a single egui version and adapted to egui 0.35 via `eframe::egui`. The theme
//! data lives in `themes.rs`, generated upstream from `themes.rs.tera` (kept alongside
//! for provenance).
//!
//! To use, call [`set_theme`] with the egui context and a [`Theme`].

use eframe::egui;
use eframe::egui::{epaint, style};

mod themes;
pub use themes::*;

/// Apply the given theme to a [`Context`](egui::Context).
pub fn set_theme(ctx: &egui::Context, theme: Theme) {
    let old = ctx.global_style().visuals.clone();
    ctx.set_visuals(theme.visuals(old));
}

fn make_widget_visual(
    old: style::WidgetVisuals,
    theme: &Theme,
    bg_fill: egui::Color32,
) -> style::WidgetVisuals {
    style::WidgetVisuals {
        bg_fill,
        weak_bg_fill: bg_fill,
        bg_stroke: egui::Stroke {
            color: theme.overlay1,
            ..old.bg_stroke
        },
        fg_stroke: egui::Stroke {
            color: theme.text,
            ..old.fg_stroke
        },
        ..old
    }
}

impl Theme {
    fn visuals(&self, old: egui::Visuals) -> egui::Visuals {
        let is_latte = *self == LATTE;
        let shadow_color = if is_latte {
            egui::Color32::from_black_alpha(25)
        } else {
            egui::Color32::from_black_alpha(96)
        };

        egui::Visuals {
            hyperlink_color: self.rosewater,
            faint_bg_color: self.surface0,
            extreme_bg_color: self.crust,
            code_bg_color: self.mantle,
            warn_fg_color: self.peach,
            error_fg_color: self.maroon,
            window_fill: self.base,
            panel_fill: self.base,
            window_stroke: egui::Stroke {
                color: self.overlay1,
                ..old.window_stroke
            },
            widgets: style::Widgets {
                noninteractive: make_widget_visual(old.widgets.noninteractive, self, self.base),
                inactive: make_widget_visual(old.widgets.inactive, self, self.surface0),
                hovered: make_widget_visual(old.widgets.hovered, self, self.surface2),
                active: make_widget_visual(old.widgets.active, self, self.surface1),
                open: make_widget_visual(old.widgets.open, self, self.surface0),
            },
            selection: style::Selection {
                bg_fill: self.blue.linear_multiply(if is_latte { 0.4 } else { 0.2 }),
                stroke: egui::Stroke {
                    color: self.text,
                    ..old.selection.stroke
                },
            },
            window_shadow: epaint::Shadow {
                color: shadow_color,
                ..old.window_shadow
            },
            popup_shadow: epaint::Shadow {
                color: shadow_color,
                ..old.popup_shadow
            },
            dark_mode: !is_latte,
            ..old
        }
    }
}
