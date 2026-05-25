use ratatui::style::Color;

// Catppuccin Mocha Color Palette for premium aesthetic dark mode styling
pub const COLOR_BASE: Color = Color::Rgb(30, 30, 46);      // Dark grey base background

pub const COLOR_SURFACE0: Color = Color::Rgb(49, 50, 68);  // Surface dark grey
pub const COLOR_SURFACE1: Color = Color::Rgb(67, 76, 94);  // Elevated active surface
pub const COLOR_TEXT: Color = Color::Rgb(205, 214, 244);   // Soft white text
pub const COLOR_SUBTEXT: Color = Color::Rgb(166, 172, 200); // Muted grey text

pub const COLOR_MAUVE: Color = Color::Rgb(203, 166, 247);  // Light Purple / Primary theme
pub const COLOR_TEAL: Color = Color::Rgb(148, 226, 213);   // Soft cyan
pub const COLOR_GREEN: Color = Color::Rgb(166, 227, 161);  // Vivid green (Success, Play, active)
pub const COLOR_PEACH: Color = Color::Rgb(250, 179, 135);  // Soft orange
pub const COLOR_RED: Color = Color::Rgb(243, 139, 168);    // Vivid red (Record, Solo, warning)
pub const COLOR_BLUE: Color = Color::Rgb(137, 180, 250);   // Calm blue
pub const COLOR_YELLOW: Color = Color::Rgb(249, 226, 175); // Vivid yellow

pub const TRACK_COLORS: [Color; 4] = [
    COLOR_MAUVE,
    COLOR_TEAL,
    COLOR_BLUE,
    COLOR_PEACH,
];
