use crate::app::{App, PlanningForm, PlanningStage};
use crate::settings::Theme;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Position, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap},
};
use std::time::{Duration, Instant};
const ASCII_LOGO: [&str; 8] = [
    "    ███████               ███████████                              █████     ",
    "  ███▒▒▒▒▒███            ▒█▒▒▒███▒▒▒█                             ▒▒███      ",
    " ███     ▒▒███ ████████  ▒   ▒███  ▒  ████████   ██████    ██████  ▒███ █████",
    "▒███      ▒███▒▒███▒▒███     ▒███    ▒▒███▒▒███ ▒▒▒▒▒███  ███▒▒███ ▒███▒▒███ ",
    "▒███      ▒███ ▒███ ▒███     ▒███     ▒███ ▒▒▒   ███████ ▒███ ▒▒▒  ▒██████▒  ",
    "▒▒███     ███  ▒███ ▒███     ▒███     ▒███      ███▒▒███ ▒███  ███ ▒███▒▒███ ",
    " ▒▒▒███████▒   ████ █████    █████    █████    ▒▒████████▒▒██████  ████ █████",
    "   ▒▒▒▒▒▒▒    ▒▒▒▒ ▒▒▒▒▒    ▒▒▒▒▒    ▒▒▒▒▒      ▒▒▒▒▒▒▒▒  ▒▒▒▒▒▒  ▒▒▒▒ ▒▒▒▒▒ ",
];
const SPLASH_DURATION: Duration = Duration::from_millis(1200);
const EGG_ART: [&str; 51] = [
    "⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢸⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
    "⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢀⣀⣀⣀⣀⣀⣀⣼⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
    "⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠻⠿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣽⣭⣭⣭⣭⣭⣭⣿⣿⣷⣶⣶⣶⣤⣤⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
    "⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠈⣹⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣟⠋⠉⠉⠁⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
    "⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣠⢞⡻⣽⣻⢿⡿⣿⢿⡿⣿⠿⡿⠿⠿⠟⡿⠿⠿⠟⠓⢄⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
    "⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣠⠞⢡⠊⡕⠢⠙⠊⠓⠉⠀⠁⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠑⠄⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
    "⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣴⠋⠄⢃⠠⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠈⠢⡀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
    "⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢀⡞⢁⠢⠘⠠⠐⠈⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠘⠄⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
    "⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢀⡏⢰⠈⡆⢉⢀⠁⠆⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠈⢆⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
    "⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢠⡞⢠⠃⡜⢠⠃⡰⢈⠠⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠈⢂⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
    "⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢠⡏⢰⢡⠚⡌⢆⡱⠐⢂⠔⡀⠂⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢃⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
    "⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢀⡞⣌⠣⣎⠳⣉⠖⠤⡉⠆⡄⠐⠠⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢆⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
    "⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⡾⠰⣌⠳⣌⠳⡥⣊⠵⣈⠒⡠⠁⠄⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠈⢄⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
    "⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣸⠣⡝⣬⠳⣬⢳⡱⢥⠒⡤⢃⠔⠡⠈⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢀⣠⣴⠾⠿⠿⢶⣄⠀⠀⠀⠀⠀⠀⠀⠘⡄⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
    "⠀⠀⠀⠀⠀⠀⠀⠀⠀⢠⡏⡵⣙⢦⣛⡴⣣⣷⣮⣷⣰⡣⡜⡠⢁⠈⠀⠀⠀⠀⠀⠀⠀⠀⣠⡴⠛⢉⣀⠤⠀⠀⠀⠉⠳⡄⠀⠀⠀⠀⠀⠀⢱⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
    "⠀⠀⠀⠀⠀⠀⠀⠀⠀⣾⡘⢶⣩⢶⣻⣾⣿⣿⣿⣿⣿⣿⣿⣷⣥⣊⡄⢁⠀⡄⠐⣠⣴⠞⣡⡶⠟⠉⠀⠀⠀⠀⠀⠀⠀⠙⠂⠀⠀⠀⠀⠀⠀⡆⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
    "⠀⠀⠀⠀⠀⠀⠀⠀⢰⢧⡙⣶⣹⣾⣿⡟⡿⣽⡿⣏⠉⠙⠛⢿⣿⣿⣿⣷⣷⡇⠙⠫⣃⠈⢴⣾⠁⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠸⡀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
    "⠀⠀⠀⠀⠀⠀⠀⠀⣼⢢⡝⡲⣟⠫⠑⠈⠙⣿⣽⣾⣷⣶⣶⣤⣝⣿⣾⢯⣿⠇⠀⠀⠀⠠⣿⣿⣿⣿⣿⡗⠢⡄⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⡇⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
    "⠀⠀⠀⠀⠀⠀⠀⢀⡟⢦⡙⠵⡀⠂⠀⠀⠀⠈⠻⣿⣿⣿⣿⣏⠜⠻⠟⣿⠟⠀⠀⠀⠀⠀⢹⣿⣿⣿⣿⠧⠚⠁⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢱⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
    "⠀⠀⠀⠀⠀⠀⠀⠸⣟⢢⣙⢢⡑⡬⢠⢀⠀⠀⠀⠀⠉⠉⠋⠉⠀⠀⢈⣿⣁⠀⠀⠀⠀⠀⠀⠈⠉⠉⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢸⡀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
    "⠀⠀⠀⠀⠀⠀⠀⢸⣇⠳⣬⣳⣽⡳⣏⣦⢃⡆⣄⡀⢀⠀⠀⠀⣀⣤⣾⣿⣿⣷⣦⣄⡀⢰⣦⣤⣀⣀⢀⣀⣀⣀⠀⡀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⡇⠀⠀⠀⠀⠀⠀⠀⠀⠀",
    "⠀⠀⠀⠀⠀⠀⠀⢸⣜⡳⣧⣟⡾⣽⣻⡼⣯⣿⣟⣿⣿⣿⣿⣿⢿⡻⣿⣿⣿⣾⣿⣿⠿⠊⠉⠛⠻⠿⠿⢾⣷⣯⣟⣶⡡⢀⠀⠀⠀⠀⠀⠀⠀⠀⠀⡇⠀⠀⠀⠀⠀⠀⠀⠀⠀",
    "⠀⠀⠀⠀⠀⠀⠀⢸⣎⣷⣻⢾⣻⡽⣷⣛⠦⠙⢿⡟⣿⣾⣝⡮⢓⠙⠓⠉⠉⠉⢣⠀⠀⠀⠀⠀⠀⢀⣀⠴⠈⠙⢛⠮⡓⢌⠠⠀⠀⠀⠀⠀⠀⠀⠀⡇⠀⠀⠀⠀⠀⠀⠀⠀⠀",
    "⠀⠀⠀⠀⠀⠀⠀⣸⣟⣮⣟⡿⣽⣻⢷⣏⠷⡀⠀⠹⣶⢯⡿⣿⣿⣾⣶⣶⣶⣤⣤⣴⣶⡶⠿⢟⣛⡿⠁⠀⠀⠀⠀⠂⠡⢂⠂⠄⠀⠀⠀⠀⠀⠀⠀⢳⡀⠀⠀⠀⠀⠀⠀⠀⠀",
    "⠀⠀⠀⠀⠀⠀⢾⣿⡽⣾⣽⣻⣽⣯⡿⣞⣯⢵⢢⢄⠩⣛⢾⣳⢯⣟⡿⣷⣾⣭⣽⣴⣦⣶⠿⠋⠁⠀⠀⠀⠀⠀⠀⠈⠀⠄⠀⠂⠀⠀⠀⠀⠀⠀⠀⢸⣿⠀⠀⠀⠀⠀⠀⠀⠀",
    "⠀⠀⠀⠀⠀⠀⢸⣿⣹⢷⣏⡿⣷⣏⣿⢿⡾⣏⣷⡎⣷⠈⢶⠉⡾⢉⠿⡉⠿⠹⠏⠉⠉⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣸⣿⡆⠀⠀⠀⠀⠀⠀⠀",
    "⠀⠀⠀⠀⠀⠀⣿⣿⣯⣟⣾⢿⣯⢿⣾⣻⢿⡽⣾⡽⡾⣝⡦⡑⡄⠃⡄⠁⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣿⠏⢣⠀⠀⠀⠀⠀⠀⠀",
    "⠀⠀⠀⠀⠀⣼⣟⣿⣿⣾⢯⣿⢾⣟⣷⣯⢿⣻⢷⣻⣽⢯⡷⣝⢦⢣⡐⠌⡀⢀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢰⣿⠁⣠⣦⠀⠀⠀⠀⠀⠀",
    "⠀⠀⠀⠀⠸⣿⣟⡾⣿⣿⣟⣯⣿⣟⣾⡽⣿⣽⣻⡽⣾⢯⣟⣯⣟⣧⡝⣦⣑⢂⠆⡠⢀⠀⡀⢀⠀⡀⢀⠀⡀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢠⣿⠏⠀⣿⣿⠇⠀⠀⠀⠀⠀",
    "⠀⠀⢀⣤⣾⣿⣿⣿⢽⣿⣿⣿⣽⣾⣯⢿⣟⣾⣽⣻⣽⣻⣯⢿⣾⢯⣿⢶⣯⡞⣽⣰⢃⡮⣐⡆⢦⡱⢈⡐⠠⠀⠄⡀⠄⠠⠀⢀⠀⠀⠀⡀⢤⣿⠃⢀⣴⣿⣿⣷⣤⡀⠀⠀⠀",
    "⠀⣴⣿⣿⣿⣿⠿⢻⣧⣛⣿⣿⣿⣷⣿⣟⣿⢾⣳⣟⡾⣷⣻⣯⢿⣻⣽⡿⣞⣿⡷⣯⣟⡾⢧⣛⠣⢆⠡⠄⠡⡈⠄⡐⠠⢀⠡⠀⠠⢈⠐⣸⡿⠋⠀⡾⠛⠿⣿⣿⣿⣿⣦⡀⠀",
    "⣾⣿⣿⣿⠋⠁⠀⠀⢿⣿⣮⠹⢿⣿⣿⣿⣯⣿⡿⣽⣿⡽⣷⣻⣟⣯⡷⣿⣻⢷⡻⢯⠽⣍⡳⣌⠣⢎⡔⢩⠐⡁⠆⡐⣁⠂⠤⢡⢁⣦⡿⠋⠀⣠⡾⠀⠀⠀⠈⠛⢿⣿⣿⣿⣆",
    "⠛⣿⣿⣿⣷⣄⡀⠀⠈⢿⣿⣷⣮⣭⡿⣿⣿⣿⣿⣿⣾⣿⢿⣽⣾⢯⣟⡷⣯⢷⣻⣭⢻⡼⣱⢭⡙⣦⡘⡥⢚⡄⣓⠰⢤⣉⣶⡵⠛⠁⠀⠀⣠⣿⠁⠀⠀⠀⠀⣠⣾⣿⣯⣿⠟",
    "⠀⠀⠙⠿⣿⣿⣿⣶⣤⣸⣿⣿⣿⣿⣷⣭⢟⡻⢿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣽⣯⢷⣯⢷⣝⣳⣎⢷⡲⣝⡼⣣⣞⣴⠿⠖⠋⠁⢀⣤⣤⣶⣿⣿⠃⠀⣀⣤⣶⣿⣿⣿⡷⠛⠁⠀",
    "⠀⠀⠀⠀⠈⠛⢿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣯⣷⣷⣮⣽⣝⡻⢛⠛⠿⠿⠿⠿⢿⡿⣿⠿⠾⠷⠿⠟⠛⠛⠛⠉⠁⠀⠀⠀⣀⣤⣿⣿⣿⣿⣿⣷⣶⣿⣿⣿⣿⡷⠛⠉⠀⠀⠀⠀",
    "⠀⠀⠀⠀⠀⠀⠀⠙⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣶⣦⣤⣠⣀⣀⣀⣀⣠⣶⣦⣄⣄⣤⣤⣶⣾⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⡿⠋⠀⠀⠀⠀⠀⠀⠀",
    "⠀⠀⠀⠀⠀⠀⠀⠀⠙⠋⠀⢹⣿⣿⣿⣿⣿⠿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⠟⢿⣿⣿⡟⠉⠻⠏⠀⠀⠀⠀⠀⠀⠀⠀⠀",
    "⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠉⠀⠀⠙⢿⣧⠈⠙⠿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⡿⠟⠋⢹⣿⠁⠀⠀⠉⠉⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
    "⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠻⠇⠀⠀⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⠃⠀⠀⠘⠋⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
    "⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢸⣿⣿⣿⣿⠇⠀⠀⠀⠉⠉⠉⠉⠉⠀⠀⠀⢸⣿⣿⣿⣿⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
    "⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣿⣿⣿⣿⣿⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢸⣿⣿⣿⣛⠄⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
    "⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠘⣿⣿⣿⣿⡧⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢿⣿⣿⣿⡟⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
    "⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣿⣿⣿⣿⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢈⣿⣿⣿⡇⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
    "⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣿⣿⣿⡏⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢨⣿⣿⣾⠁⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
    "⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢀⣿⡿⢹⠃⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠐⣿⣿⣿⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
    "⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢸⣿⠇⢸⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢰⣿⣿⣟⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
    "⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣾⣟⠀⢺⡀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢀⣾⣿⡿⣿⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
    "⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣀⡼⠋⠀⠳⡄⠱⣄⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢰⣿⣿⣿⣁⠸⠷⣄⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
    "⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣀⣤⣶⣉⠉⠁⠤⢀⣠⣶⣆⣼⠆⠀⠀⠀⠀⠀⠀⠀⠀⠀⠈⣿⣿⣿⣿⣦⣴⣶⣿⣦⣄⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
    "⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢀⣴⣿⣿⡿⣿⣿⣷⣶⣶⡿⠿⢿⡿⠇⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠈⠁⠉⠛⢿⣿⣿⣿⣿⣿⣷⡄⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
    "⠀⠀⠀⠀⠀⠀⠀⠀⠀⣴⣿⣿⣿⢻⣽⣿⣿⠟⠋⠁⡀⠀⠀⡀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠈⢿⣿⣿⣿⣻⣿⢿⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
];
const GIRL_FRAMES: [[&str; 10]; 3] = [
    [
        "          .-\"\"\"-.          ",
        "        .'  .-.  '.        ",
        "       /  //   \\  \\       ",
        "      |  | o   o |  |      ",
        "      |  |   ^   |  |      ",
        "      |  |  \\_/  |  |      ",
        "       \\  \\_____/  /  \\o   ",
        "        '-._|_|_.-'   /    ",
        "           /|\\      /     ",
        "          /_|_\\           ",
    ],
    [
        "          .-\"\"\"-.       *  ",
        "        .'  .-.  '.     o/ ",
        "       /  //   \\  \\    /  ",
        "      |  | o   o |  |  /   ",
        "      |  |   ^   |  | /    ",
        "      |  |  \\_/  |  |/     ",
        "       \\  \\_____/  /      ",
        "        '-._|_|_.-'       ",
        "           /|\\            ",
        "          /_|_\\           ",
    ],
    [
        "          .-\"\"\"-.          ",
        "        .'  .-.  '.    *   ",
        "       /  //   \\  \\   o/  ",
        "      |  | -   - |  | /    ",
        "      |  |   ^   |  |/     ",
        "      |  |  \\_/  |  |      ",
        "       \\  \\_____/  /      ",
        "        '-._|_|_.-'       ",
        "           /|\\            ",
        "          /_|_\\           ",
    ],
];

fn girl_frame(elapsed: Duration) -> &'static [&'static str; 10] {
    &GIRL_FRAMES[(elapsed.as_millis() / 300 % GIRL_FRAMES.len() as u128) as usize]
}

/// Return the next instant at which rendering can visibly change without input.
pub fn next_visual_update(app: &App, start: Instant, now: Instant) -> Option<Instant> {
    let mut next = None;
    let mut schedule = |deadline: Instant| {
        if deadline > now {
            next = Some(next.map_or(deadline, |current: Instant| current.min(deadline)));
        }
    };

    if !app.easter_egg
        && app.modal.is_none()
        && app.help.is_none()
        && now.saturating_duration_since(start) < SPLASH_DURATION
    {
        let elapsed = now.saturating_duration_since(start);
        let frame = elapsed.as_millis() / 300;
        schedule(start + Duration::from_millis(((frame + 1) * 300) as u64));
        schedule(start + SPLASH_DURATION);
    }

    if let Some(animation) = app
        .drawer_animation
        .as_ref()
        .filter(|_| app.settings.animate_drawers)
    {
        let end = animation.started + Duration::from_millis(220);
        if now < end {
            // Match the event loop's 35ms animation cadence, plus always schedule its final frame.
            let elapsed = now.saturating_duration_since(animation.started);
            let step = elapsed.as_millis() / 35 + 1;
            schedule((animation.started + Duration::from_millis((step * 35) as u64)).min(end));
            schedule(end);
        }
    }

    if let Some(expires_at) = app.toast.as_ref().and_then(|toast| toast.expires_at) {
        schedule(expires_at);
    }
    next
}

fn splash_lines(
    elapsed: Duration,
    area: Rect,
    teal: Color,
    gold: Color,
    muted: Color,
) -> Vec<Line<'static>> {
    let girl = girl_frame(elapsed);
    // One width for every frame keeps the character fixed in place as she moves.
    let girl_width = GIRL_FRAMES
        .iter()
        .flatten()
        .map(|line| Span::raw(*line).width())
        .max()
        .unwrap_or(0);
    let logo_width = ASCII_LOGO
        .iter()
        .map(|line| Span::raw(*line).width())
        .max()
        .unwrap_or(0);
    let large = usize::from(area.width) >= logo_width
        && usize::from(area.height) >= ASCII_LOGO.len() + 1 + girl.len();
    let show_girl = usize::from(area.width) >= girl_width
        && usize::from(area.height) >= (if large { ASCII_LOGO.len() } else { 2 }) + 1 + girl.len();

    let mut lines = if large {
        ASCII_LOGO
            .iter()
            .map(|line| {
                Line::from(Span::styled(
                    *line,
                    Style::default().fg(teal).add_modifier(Modifier::BOLD),
                ))
            })
            .collect::<Vec<_>>()
    } else {
        vec![
            Line::from(Span::styled(
                "ONTRACK",
                Style::default().fg(teal).add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                "BEADS WORKSPACE BROWSER",
                Style::default().fg(muted),
            )),
        ]
    };
    if show_girl {
        lines.push(Line::raw(""));
        lines.extend(girl.iter().enumerate().map(|(index, line)| {
            Line::from(Span::styled(
                format!("{line:<girl_width$}"),
                Style::default().fg(if index <= 2 { gold } else { teal }),
            ))
        }));
    }
    lines
}
fn row_issue_type(bead: &crate::beads::Bead) -> String {
    ["issue_type", "type"]
        .into_iter()
        .find_map(|field| {
            bead.data
                .get(field)
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|kind| !kind.is_empty())
        })
        .map(str::to_owned)
        .unwrap_or_else(|| "unknown".to_owned())
}
fn settings_hint(dropdown_open: bool, focus: usize) -> &'static str {
    if dropdown_open {
        "↑ / ↓ move · ← / → choice · Enter / Space select · Tab moves · Esc close"
    } else if focus == 8 {
        "↑ / ↓ move · Enter / Space close · Tab moves · Esc close"
    } else if focus >= 3 {
        "↑ / ↓ move · Enter / Space toggle · Tab moves · Esc close"
    } else {
        "↑ / ↓ move · ← / → choice · Enter / Space open · Tab moves"
    }
}
fn settings_columns(width: usize) -> (usize, usize) {
    let title = width * 2 / 3;
    (title, width - title)
}
fn settings_row(
    label: &str,
    value: &str,
    width: usize,
    label_style: Style,
    value_style: Style,
) -> Line<'static> {
    let (title_width, value_width) = settings_columns(width);
    let title = truncate_display(label, title_width);
    let title_padding = title_width.saturating_sub(Span::raw(title.as_str()).width());
    let value = truncate_display(value, value_width);
    let value_padding = value_width.saturating_sub(Span::raw(value.as_str()).width());
    let left_padding = value_padding / 2;
    Line::from(vec![
        Span::styled(format!("{title}{}", " ".repeat(title_padding)), label_style),
        Span::styled(
            format!(
                "{}{}{}",
                " ".repeat(left_padding),
                value,
                " ".repeat(value_padding - left_padding)
            ),
            value_style,
        ),
    ])
}
const MAX_VISIBLE_THEMES: usize = 6;
fn visible_theme_window(theme: Theme, available_height: u16) -> (usize, usize) {
    let total = crate::theme::CATALOG.len();
    let visible = usize::from(available_height)
        .saturating_sub(22)
        .clamp(1, MAX_VISIBLE_THEMES.min(total));
    let selected = Theme::ALL
        .iter()
        .position(|choice| *choice == theme)
        .unwrap_or(0);
    let start = selected
        .saturating_sub(visible.saturating_sub(1))
        .min(total.saturating_sub(visible));
    (start, visible)
}
fn visible_agent_window(
    agent: crate::settings::DefaultAgent,
    available_height: u16,
) -> (usize, usize) {
    let total = crate::beads::HARNESSES.len() + 2;
    let visible = usize::from(available_height).saturating_sub(23).clamp(1, 6);
    let start = agent
        .index()
        .saturating_sub(visible.saturating_sub(1))
        .min(total - visible);
    (start, visible)
}
fn visible_limit_window(limit: crate::settings::AgentLimit, available_height: u16) -> (u64, usize) {
    let visible = usize::from(available_height)
        .saturating_sub(23)
        .clamp(1, MAX_VISIBLE_THEMES);
    let selected = match limit {
        crate::settings::AgentLimit::Unlimited => 0,
        crate::settings::AgentLimit::Limited(count) => u64::from(count),
    };
    (selected.saturating_sub((visible - 1) as u64), visible)
}
fn render_header(f: &mut Frame, area: Rect, left: Line<'static>, app: &App) {
    let palette = crate::theme::definition(app.theme).palette;
    f.render_widget(
        Paragraph::new(left).style(Style::default().fg(palette.foreground)),
        area,
    );
}
fn render_header_feedback(f: &mut Frame, area: Rect, message: Option<&str>, theme: Theme) {
    let Some(message) = message else {
        return;
    };
    let palette = crate::theme::definition(theme).palette;
    f.render_widget(
        Paragraph::new(truncate_display(
            message,
            area.width.saturating_sub(1) as usize,
        ))
        .style(Style::default().fg(palette.warning).bg(palette.background)),
        Rect::new(area.x + 1, area.y + 1, area.width.saturating_sub(1), 1),
    );
}
pub fn render(f: &mut Frame, elapsed: Duration, app: &mut App) {
    let a = f.area();
    let palette = crate::theme::definition(app.theme).palette;
    debug_assert_eq!(
        app.theme.is_light(),
        crate::theme::definition(app.theme).is_light
    );
    let bg = palette.background;
    let ink = palette.foreground;
    let muted = palette.muted;
    let teal = palette.accent;
    let gold = palette.warning;
    f.render_widget(Block::default().style(Style::default().bg(bg)), a);
    app.list_rect = Rect::default();
    app.detail_rect = Rect::default();
    app.board_columns.clear();
    app.board_cards.clear();
    if app.easter_egg {
        app.modal_ready = false;
        let lines = if a.width >= 65 && a.height >= EGG_ART.len() as u16 + 2 {
            EGG_ART
                .iter()
                .map(|line| Line::from(Span::styled(*line, Style::default().fg(teal))))
                .chain([
                    Line::raw(""),
                    Line::from(Span::styled("Esc to return", Style::default().fg(muted))),
                ])
                .collect::<Vec<_>>()
        } else if a.width >= 3 && a.height >= 1 {
            vec![Line::from(Span::styled("(o)", Style::default().fg(teal)))]
        } else {
            Vec::new()
        };
        let height = lines.len() as u16;
        if height > 0 {
            f.render_widget(
                Paragraph::new(lines).alignment(Alignment::Center),
                Rect::new(
                    a.x,
                    a.y + a.height.saturating_sub(height) / 2,
                    a.width,
                    height,
                ),
            );
        }
        return;
    }
    if a.width < 24 || a.height < 7 {
        app.modal_ready = false;
        f.render_widget(
            Paragraph::new(if app.modal.is_some() {
                "Resize terminal"
            } else {
                "ontrack · expand terminal"
            })
            .style(Style::default().fg(muted))
            .alignment(Alignment::Center),
            a,
        );
        render_toast(f, app);
        render_help(f, app);
        return;
    }
    if let Some(crate::app::Modal::Planning(form)) = &app.modal {
        app.modal_ready = a.width >= 50 && a.height >= 12;
        render_planning(f, a, form, app.theme, app.modal_ready);
        render_help(f, app);
        return;
    }
    if app.modal.is_none() && app.help.is_none() && elapsed < SPLASH_DURATION {
        let lines = splash_lines(elapsed, a, teal, gold, muted);
        let height = lines.len() as u16;
        f.render_widget(
            Paragraph::new(lines)
                .alignment(Alignment::Center)
                .style(Style::default().fg(ink)),
            Rect::new(
                a.x,
                a.y + a.height.saturating_sub(height) / 2,
                a.width,
                height,
            ),
        );
        render_toast(f, app);
        return;
    }
    let v = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(3),
            Constraint::Length(1),
        ])
        .split(a);
    render_header(
        f,
        v[0],
        Line::from(vec![
            Span::styled(
                " BEADS ",
                Style::default()
                    .fg(palette.selected_foreground)
                    .bg(palette.selected_background)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  ONTRACK  /  WORKSPACE ISSUES"),
        ]),
        app,
    );
    let feedback = if app.worktrees_view && app.worktrees_error.is_some() {
        app.worktrees_error
            .as_ref()
            .map(|error| format!("WORKTREE ERROR · {error}"))
    } else if app.worktrees_view && app.worktrees_focus_feedback.is_some() {
        app.worktrees_focus_feedback
            .as_ref()
            .map(|feedback| match feedback {
                crate::app::WorktreeFocusFeedback::Success(message) => message.clone(),
                crate::app::WorktreeFocusFeedback::Error(message) => {
                    format!("WORKSPACE ERROR · {message}")
                }
            })
    } else if let Some(error) = &app.list_error {
        Some(format!("LIST ERROR · {error}"))
    } else if let Some(error) = &app.detail_error {
        Some(format!("DETAIL ERROR · {error}"))
    } else {
        app.notice.clone()
    };
    render_header_feedback(f, v[0], feedback.as_deref(), app.theme);
    render_kanban(f, v[1], app);
    render_drawer(f, a, app, Instant::now());
    f.render_widget(
        Paragraph::new(footer_text(v[2].width as usize))
            .style(Style::default().fg(muted).bg(palette.panel_alt)),
        v[2],
    );
    if let Some(modal) = &app.modal {
        // Recolor the complete rendered view, including drawers, before the
        // dialog's Clear and normal themed rendering restore its own styles.
        dim_background(f, a);
        let width = match modal {
            crate::app::Modal::Create(_) | crate::app::Modal::Edit { .. } => {
                FORM_DESCRIPTION_WIDTH as u16 + 6
            }
            crate::app::Modal::Settings { .. } => 64,
            _ => 62,
        }
        .min(a.width.saturating_sub(2));
        let confirmation_lines = match modal {
            crate::app::Modal::ConfirmGenerateEpic { id, title } => Some(wrap_words(
                &format!(
                    "Epic: {id}\n{title}\n\nGenerate child tasks from the live description using a Herdr agent?\nExisting children remain; this may add duplicate tasks.\n\ny confirm · n / Enter / Esc cancel{}",
                    app.notice
                        .as_ref()
                        .map(|text| format!("\n\n{text}"))
                        .unwrap_or_default()
                ),
                width.saturating_sub(4) as usize,
            )),
            crate::app::Modal::ConfirmClose { id, title } => Some(if app.mutation_busy {
                vec![
                    format!("Closing bead {id}…"),
                    "Command running · dialog locked".into(),
                ]
            } else {
                wrap_words(
                    &format!(
                        "Close this bead?\nID: {id}\n{}\n\nOnly this issue will be closed.\nVerified Git integration is required first.\n\n[y] Close · [n / Enter / Esc] Cancel{}",
                        truncate_display(title, width.saturating_sub(4) as usize),
                        app.notice
                            .as_ref()
                            .map(|text| format!("\n\n{text}"))
                            .unwrap_or_default()
                    ),
                    width.saturating_sub(4) as usize,
                )
            }),
            crate::app::Modal::ConfirmDelete { id, title } => Some(delete_dialog_lines(
                id,
                title,
                width.saturating_sub(4) as usize,
                app.notice.as_deref(),
                app.mutation_busy,
            )),
            _ => None,
        };
        let height = match modal {
            crate::app::Modal::Planning(_) => 0,
            crate::app::Modal::Settings {
                error,
                dropdown_open,
                draft,
                focus,
            } => {
                let error_lines = error
                    .as_deref()
                    .map(|message| wrap_words(message, width.saturating_sub(4) as usize).len())
                    .unwrap_or(0);
                let hint_lines = wrap_words(
                    settings_hint(*dropdown_open, *focus),
                    width.saturating_sub(4) as usize,
                )
                .len();
                let option_lines = if *dropdown_open {
                    if *focus == 0 {
                        let (start, count) = visible_theme_window(draft.theme, a.height);
                        count
                            + usize::from(start > 0)
                            + usize::from(start + count < crate::theme::CATALOG.len())
                    } else if *focus == 1 {
                        let (start, count) = visible_agent_window(draft.default_agent, a.height);
                        count
                            + usize::from(start > 0)
                            + usize::from(start + count < crate::beads::HARNESSES.len() + 2)
                    } else if *focus == 2 {
                        let (start, count) = visible_limit_window(draft.agent_limit, a.height);
                        count
                            + usize::from(start > 0)
                            + usize::from(start + count as u64 <= u32::MAX as u64)
                    } else {
                        0
                    }
                } else {
                    0
                };
                (11 + option_lines + error_lines + hint_lines + 2) as u16
            }
            crate::app::Modal::Create(form) | crate::app::Modal::Edit { form, .. } => {
                if form.dropdown.is_some() { 22 } else { 18 }
            }
            crate::app::Modal::EditLoading { .. } => 8,
            crate::app::Modal::ReviewResult { .. } => 15,
            crate::app::Modal::Move { .. } => 10,
            crate::app::Modal::ConfirmGenerateEpic { .. }
            | crate::app::Modal::ConfirmClose { .. }
            | crate::app::Modal::ConfirmDelete { .. } => confirmation_lines
                .as_ref()
                .map_or(10, |lines| (lines.len() + 2).max(10) as u16),
            crate::app::Modal::ConfirmWorktree { action: 3 | 5, .. } => 12,
            crate::app::Modal::ConfirmWorktree { action: 4, .. } => 11,
            crate::app::Modal::ConfirmWorktree { .. } => 8,
        };
        let min_width = if matches!(
            modal,
            crate::app::Modal::Create(_) | crate::app::Modal::Edit { .. }
        ) {
            70
        } else if matches!(
            modal,
            crate::app::Modal::Settings { .. } | crate::app::Modal::EditLoading { .. }
        ) {
            38
        } else {
            50
        };
        if a.width < min_width || a.height < height {
            app.modal_ready = false;
            f.render_widget(Clear, a);
            let escape_hint = if matches!(modal, crate::app::Modal::Settings { .. }) {
                "Esc closes"
            } else {
                "Esc cancels"
            };
            let prompt = if a.width < min_width {
                format!("Expand terminal width to {min_width} columns · {escape_hint}")
            } else {
                format!("Expand terminal height to {height} rows · {escape_hint}")
            };
            f.render_widget(
                Paragraph::new(prompt)
                    .style(Style::default().fg(gold))
                    .alignment(Alignment::Center),
                a,
            );
            render_toast(f, app);
            render_help(f, app);
            return;
        }
        app.modal_ready = true;
        let area = Rect::new(
            a.x + a.width.saturating_sub(width) / 2,
            a.y + a.height.saturating_sub(height) / 2,
            width,
            height,
        );
        f.render_widget(Clear, area);
        let frame = Block::default()
            .title(match modal {
                crate::app::Modal::Create(form) if form.parent_id.is_some() => " NEW CHILD TASK ",
                crate::app::Modal::Create(_) => " NEW BEAD ",
                crate::app::Modal::EditLoading { .. } | crate::app::Modal::Edit { .. } => {
                    " EDIT BEAD "
                }
                crate::app::Modal::Move { .. } => " MOVE BEAD ",
                crate::app::Modal::ConfirmGenerateEpic { .. } => " GENERATE EPIC TASKS? ",
                crate::app::Modal::ConfirmClose { .. } => " CLOSE BEAD? ",
                crate::app::Modal::ReviewResult { accept: true, .. } => " ACCEPT RESULT ",
                crate::app::Modal::ReviewResult { .. } => " REQUEST CHANGES ",
                crate::app::Modal::Settings { .. } => " SETTINGS ",
                crate::app::Modal::ConfirmWorktree { action: 3, .. } => " RESTART TASK? ",
                crate::app::Modal::ConfirmWorktree { action: 4, .. } => " CONTINUE TASK? ",
                crate::app::Modal::ConfirmWorktree { action: 5, .. } => " FORCE REMOVE WORKTREE? ",
                crate::app::Modal::ConfirmWorktree { .. } => " CONFIRM WORKTREE ACTION ",
                _ => " DELETE BEAD? ",
            })
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(palette.border))
            .style(Style::default().bg(palette.panel));
        f.render_widget(frame, area);
        let inner = Rect::new(
            area.x + 2,
            area.y + 1,
            area.width.saturating_sub(4),
            area.height.saturating_sub(2),
        );
        match modal {
            crate::app::Modal::Planning(_) => {}
            crate::app::Modal::EditLoading { id, error, .. } => {
                let message = error.as_ref().map_or_else(
                    || format!("Loading {id}…"),
                    |error| format!("Could not load {id}: {error}\nPress e to retry · Esc cancels"),
                );
                f.render_widget(
                    Paragraph::new(message)
                        .wrap(Wrap { trim: false })
                        .style(Style::default().fg(if error.is_some() { gold } else { ink })),
                    inner,
                );
            }
            crate::app::Modal::Settings {
                draft,
                error,
                dropdown_open,
                focus,
            } => {
                let selected_style = Style::default()
                    .fg(palette.selected_foreground)
                    .bg(palette.selected_background)
                    .add_modifier(Modifier::BOLD);
                let value_style = Style::default().fg(teal).add_modifier(Modifier::BOLD);
                let row_width = inner.width as usize;
                let mut lines = vec![
                    Line::from("Application preferences").alignment(Alignment::Center),
                    settings_row(
                        "Theme",
                        &format!("{} ▾", draft.theme.label()),
                        row_width,
                        if *focus == 0 {
                            value_style
                        } else {
                            Style::default().fg(ink)
                        },
                        if *focus == 0 {
                            selected_style
                        } else {
                            value_style
                        },
                    ),
                ];
                if *dropdown_open && *focus == 0 {
                    let (start, count) = visible_theme_window(draft.theme, a.height);
                    let end = start + count;
                    if start > 0 {
                        lines.push(settings_row(
                            "",
                            "↑ earlier themes",
                            row_width,
                            Style::default(),
                            Style::default().fg(muted),
                        ));
                    }
                    for theme in Theme::ALL.iter().skip(start).take(count).copied() {
                        let selected = draft.theme == theme;
                        lines.push(settings_row(
                            "",
                            &format!("{} {}", if selected { "›" } else { " " }, theme.label()),
                            row_width,
                            Style::default(),
                            if selected {
                                selected_style
                            } else {
                                Style::default().fg(ink)
                            },
                        ));
                    }
                    if end < crate::theme::CATALOG.len() {
                        lines.push(settings_row(
                            "",
                            "↓ later themes",
                            row_width,
                            Style::default(),
                            Style::default().fg(muted),
                        ));
                    }
                }
                lines.push(settings_row(
                    "Default agent for new issues",
                    &format!("{} ▾", draft.default_agent.label()),
                    row_width,
                    if *focus == 1 {
                        value_style
                    } else {
                        Style::default().fg(ink)
                    },
                    if *focus == 1 {
                        selected_style
                    } else {
                        value_style
                    },
                ));
                if *dropdown_open && *focus == 1 {
                    let (start, count) = visible_agent_window(draft.default_agent, a.height);
                    if start > 0 {
                        lines.push(settings_row(
                            "",
                            "↑ earlier agents",
                            row_width,
                            Style::default(),
                            Style::default().fg(muted),
                        ));
                    }
                    for index in start..start + count {
                        let agent = crate::settings::DefaultAgent::at(index);
                        let selected = draft.default_agent == agent;
                        lines.push(settings_row(
                            "",
                            &format!("{} {}", if selected { "›" } else { " " }, agent.label()),
                            row_width,
                            Style::default(),
                            if selected {
                                selected_style
                            } else {
                                Style::default().fg(ink)
                            },
                        ));
                    }
                    if start + count < crate::beads::HARNESSES.len() + 2 {
                        lines.push(settings_row(
                            "",
                            "↓ later agents",
                            row_width,
                            Style::default(),
                            Style::default().fg(muted),
                        ));
                    }
                }
                lines.push(settings_row(
                    "Agent limit",
                    &format!("{} ▾", draft.agent_limit.label()),
                    row_width,
                    if *focus == 2 {
                        value_style
                    } else {
                        Style::default().fg(ink)
                    },
                    if *focus == 2 {
                        selected_style
                    } else {
                        value_style
                    },
                ));
                if *dropdown_open && *focus == 2 {
                    let (start, count) = visible_limit_window(draft.agent_limit, a.height);
                    if start > 0 {
                        lines.push(settings_row(
                            "",
                            "↑ earlier limits",
                            row_width,
                            Style::default(),
                            Style::default().fg(muted),
                        ));
                    }
                    for index in start..start + count as u64 {
                        let choice = if index == 0 {
                            crate::settings::AgentLimit::Unlimited
                        } else {
                            crate::settings::AgentLimit::Limited(index as u32)
                        };
                        let selected = draft.agent_limit == choice;
                        lines.push(settings_row(
                            "",
                            &format!("{} {}", if selected { "›" } else { " " }, choice.label()),
                            row_width,
                            Style::default(),
                            if selected {
                                selected_style
                            } else {
                                Style::default().fg(ink)
                            },
                        ));
                    }
                    if start + count as u64 <= u32::MAX as u64 {
                        lines.push(settings_row(
                            "",
                            "↓ later limits",
                            row_width,
                            Style::default(),
                            Style::default().fg(muted),
                        ));
                    }
                }
                for (index, (label, enabled)) in [
                    ("Auto-merge top-level tasks", draft.auto_merge),
                    (
                        "Auto-merge child tasks into parent",
                        draft.auto_merge_children,
                    ),
                    (
                        "Close Herdr workspace after integration",
                        draft.auto_close_workspace,
                    ),
                    (
                        "Remove checkout + close Herdr workspace",
                        draft.auto_remove_checkout,
                    ),
                    ("Animate drawers", draft.animate_drawers),
                ]
                .into_iter()
                .enumerate()
                {
                    let focused = *focus == index + 3;
                    lines.push(settings_row(
                        label,
                        if enabled { "[x]" } else { "[ ]" },
                        row_width,
                        if focused {
                            value_style
                        } else {
                            Style::default().fg(ink)
                        },
                        if focused {
                            selected_style
                        } else {
                            Style::default().fg(ink)
                        },
                    ));
                }
                lines.push(
                    Line::from(Span::styled(
                        "Removing a checkout also closes its Herdr workspace.",
                        Style::default().fg(muted),
                    ))
                    .alignment(Alignment::Center),
                );
                lines.push(
                    Line::from(Span::styled(
                        "[ Close ]",
                        if *focus == 8 {
                            selected_style
                        } else {
                            value_style
                        },
                    ))
                    .alignment(Alignment::Center),
                );
                if let Some(error) = error {
                    lines.extend(
                        wrap_words(error, inner.width as usize)
                            .into_iter()
                            .map(|line| {
                                Line::from(Span::styled(line, Style::default().fg(palette.danger)))
                                    .alignment(Alignment::Center)
                            }),
                    );
                }
                lines.extend(
                    wrap_words(settings_hint(*dropdown_open, *focus), inner.width as usize)
                        .into_iter()
                        .map(|line| {
                            Line::from(line)
                                .style(Style::default().fg(muted))
                                .alignment(Alignment::Center)
                        }),
                );
                f.render_widget(
                    Paragraph::new(lines)
                        .wrap(Wrap { trim: false })
                        .style(Style::default().fg(ink)),
                    inner,
                );
            }
            crate::app::Modal::Create(form) | crate::app::Modal::Edit { form, .. } => {
                let editing = matches!(modal, crate::app::Modal::Edit { .. });
                let rows = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Length(3),
                        Constraint::Length(if matches!(form.dropdown, Some(2 | 3)) {
                            7
                        } else {
                            3
                        }),
                        Constraint::Length(if form.dropdown == Some(6) { 7 } else { 3 }),
                        Constraint::Min(3),
                        Constraint::Length(0),
                        Constraint::Length(1),
                        Constraint::Length(1),
                    ])
                    .split(inner);
                let title_area = Rect::new(rows[0].x, rows[0].y, rows[0].width, 3);
                f.render_widget(
                    Paragraph::new("")
                        .block(
                            Block::default()
                                .title(match &form.parent_id {
                                    Some(parent) => format!(" Title · child of {parent} "),
                                    None => " Title ".to_owned(),
                                })
                                .borders(Borders::ALL)
                                .border_type(BorderType::Rounded)
                                .border_style(Style::default().fg(if form.focus == 0 {
                                    teal
                                } else {
                                    muted
                                })),
                        )
                        .style(Style::default().fg(ink))
                        .scroll((0, 0)),
                    title_area,
                );
                let inner_title = Rect::new(
                    title_area.x + 1,
                    title_area.y + 1,
                    title_area.width.saturating_sub(2),
                    1,
                );
                let caret_width =
                    Span::raw(&form.title[..form.cursor.min(form.title.len())]).width();
                let title_view = inner_title.width.saturating_sub(1) as usize;
                let desired_scroll = caret_width.saturating_sub(title_view.saturating_sub(1));
                let (title_slice, title_scroll) =
                    display_window(&form.title, desired_scroll, inner_title.width as usize);
                f.render_widget(
                    Paragraph::new(title_slice).style(Style::default().fg(ink)),
                    inner_title,
                );
                if form.focus == 0 {
                    f.set_cursor_position(Position::new(
                        inner_title.x
                            + caret_width
                                .saturating_sub(title_scroll)
                                .min(inner_title.width.saturating_sub(1) as usize)
                                as u16,
                        inner_title.y,
                    ));
                }
                let cols = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Percentage(65), Constraint::Percentage(35)])
                    .split(rows[1]);
                let type_area = cols[0];
                let priority_area = cols[1];
                let dropdown = form.dropdown;
                let box_height = type_area.height;
                for (which, area) in [(2usize, type_area), (3usize, priority_area)] {
                    let selected = if which == 2 {
                        form.issue_type
                    } else {
                        form.priority
                    };
                    let title = if which == 2 {
                        format!(
                            "Type: {}{}",
                            capitalize(bead_type_name(selected)),
                            if dropdown == Some(which) { "" } else { "  ▾" }
                        )
                    } else {
                        format!(
                            "Priority: P{selected}{}",
                            if dropdown == Some(which) { "" } else { "  ▾" }
                        )
                    };
                    let color = if which == 2 {
                        crate::theme::issue_type_color(
                            app.theme,
                            crate::beads::BEAD_TYPES[selected],
                        )
                    } else {
                        crate::theme::priority_color(app.theme, selected as u64)
                    };
                    let border = if form.focus == which || dropdown == Some(which) {
                        teal
                    } else {
                        muted
                    };
                    let block = Block::default()
                        .title(title)
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .border_style(Style::default().fg(border));
                    f.render_widget(block, area);
                    if dropdown == Some(which) {
                        let options = if which == 2 {
                            crate::beads::BEAD_TYPES
                                .iter()
                                .enumerate()
                                .map(|(i, x)| {
                                    (
                                        i,
                                        x.to_string(),
                                        crate::theme::issue_type_color(app.theme, x),
                                    )
                                })
                                .collect::<Vec<_>>()
                        } else {
                            (0..=4)
                                .map(|i| {
                                    (
                                        i,
                                        format!("P{i}"),
                                        crate::theme::priority_color(app.theme, i as u64),
                                    )
                                })
                                .collect()
                        };
                        let start = if which == 2 {
                            selected.saturating_sub(box_height.saturating_sub(2) as usize - 1)
                        } else {
                            0
                        };
                        for (row, (idx, label, col)) in options
                            .iter()
                            .skip(start)
                            .take(box_height.saturating_sub(2) as usize)
                            .enumerate()
                        {
                            let y = area.y + 1 + row as u16;
                            let line = Rect::new(area.x + 1, y, area.width.saturating_sub(2), 1);
                            let style = if *idx == selected {
                                Style::default()
                                    .fg(palette.selected_foreground)
                                    .bg(palette.selected_background)
                                    .add_modifier(Modifier::BOLD)
                            } else {
                                Style::default().fg(*col)
                            };
                            f.render_widget(
                                Paragraph::new(format!(
                                    "{} {label}",
                                    if *idx == selected { "›" } else { " " }
                                ))
                                .style(style),
                                line,
                            );
                        }
                    } else {
                        let line =
                            Rect::new(area.x + 1, area.y + 1, area.width.saturating_sub(2), 1);
                        f.render_widget(
                            Paragraph::new(if which == 2 {
                                capitalize(bead_type_name(selected))
                            } else {
                                format!("P{selected}")
                            })
                            .style(Style::default().fg(color)),
                            line,
                        );
                    }
                }
                let harness_area = rows[2];
                let harness_name = if form.harness == 0 {
                    "Unassigned"
                } else {
                    &form.harnesses[form.harness - 1]
                };
                let harness_border = if form.focus == 6 || form.dropdown == Some(6) {
                    teal
                } else {
                    muted
                };
                f.render_widget(
                    Block::default()
                        .title(format!("Agent harness: {harness_name}  ▾"))
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .border_style(Style::default().fg(harness_border)),
                    harness_area,
                );
                if form.dropdown == Some(6) {
                    let options = std::iter::once("Unassigned")
                        .chain(form.harnesses.iter().map(String::as_str))
                        .collect::<Vec<_>>();
                    let count = harness_area.height.saturating_sub(2) as usize;
                    let start = form.harness.saturating_sub(count.saturating_sub(1));
                    for (offset, label) in options.iter().enumerate().skip(start).take(count) {
                        let style = if offset == form.harness {
                            Style::default()
                                .fg(palette.selected_foreground)
                                .bg(palette.selected_background)
                        } else {
                            Style::default().fg(ink)
                        };
                        f.render_widget(
                            Paragraph::new(format!(
                                "{} {label}",
                                if offset == form.harness { "›" } else { " " }
                            ))
                            .style(style),
                            Rect::new(
                                harness_area.x + 1,
                                harness_area.y + 1 + (offset - start) as u16,
                                harness_area.width.saturating_sub(2),
                                1,
                            ),
                        );
                    }
                } else {
                    f.render_widget(
                        Paragraph::new(if form.harness_loading {
                            "Checking Herdr integrations…"
                        } else if form.harness_error.is_some() {
                            "Unassigned · Herdr unavailable"
                        } else if form.harnesses.is_empty() && !editing {
                            "Unassigned · no current integrations"
                        } else {
                            harness_name
                        })
                        .style(Style::default().fg(ink)),
                        Rect::new(
                            harness_area.x + 1,
                            harness_area.y + 1,
                            harness_area.width.saturating_sub(2),
                            1,
                        ),
                    );
                }
                let desc_area = rows[3];
                debug_assert_eq!(
                    desc_area.width.saturating_sub(2) as usize,
                    FORM_DESCRIPTION_WIDTH
                );
                let (wrapped_description, cx, cy) = wrapped_description(
                    &form.description,
                    form.description_cursor,
                    desc_area.width.saturating_sub(2).max(1) as usize,
                );
                let desc_scroll = cy.saturating_sub(desc_area.height.saturating_sub(3) as usize);
                f.render_widget(
                    Paragraph::new(wrapped_description)
                        .block(
                            Block::default()
                                .title(if editing {
                                    " Description · Ctrl+V image · Shift+Enter newline · Enter Save "
                                } else {
                                    " Description · Ctrl+V image · Shift+Enter newline · Enter Create "
                                })
                                .borders(Borders::ALL)
                                .border_type(BorderType::Rounded)
                                .border_style(Style::default().fg(if form.focus == 1 {
                                    teal
                                } else {
                                    muted
                                })),
                        )
                        .style(Style::default().fg(ink))
                        .scroll((desc_scroll as u16, 0)),
                    desc_area,
                );
                if form.focus == 1 {
                    let row = cy.saturating_sub(desc_scroll) as u16;
                    if row < desc_area.height.saturating_sub(2) {
                        f.set_cursor_position(Position::new(
                            desc_area.x
                                + 1
                                + cx.min(desc_area.width.saturating_sub(2) as usize) as u16,
                            desc_area.y + 1 + row,
                        ));
                    }
                }
                let actions = Paragraph::new(Line::from(vec![
                    Span::styled(
                        if editing { "[ Save ]" } else { "[ Create ]" },
                        if app.mutation_busy {
                            Style::default().fg(muted)
                        } else if form.focus == 4 {
                            Style::default()
                                .fg(palette.selected_foreground)
                                .bg(palette.selected_background)
                                .add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(teal)
                        },
                    ),
                    Span::raw("  "),
                    Span::styled(
                        "[ Cancel ]",
                        if app.mutation_busy {
                            Style::default().fg(muted)
                        } else if form.focus == 5 {
                            Style::default()
                                .fg(palette.selected_foreground)
                                .bg(gold)
                                .add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(gold)
                        },
                    ),
                    Span::raw("  ·  Enter choose · Esc cancel"),
                ]));
                f.render_widget(actions, rows[5]);
                let feedback = if app.mutation_busy {
                    Some(if editing {
                        "Saving… · form locked until the command finishes"
                    } else {
                        "Creating… · form locked until the command finishes"
                    })
                } else {
                    app.notice.as_deref().or(form.harness_error.as_deref())
                };
                let feedback = truncate_display(
                    feedback.unwrap_or(
                        "Tab moves · ←/→ cycles focused choices · Enter opens · Esc cancels",
                    ),
                    rows[6].width as usize,
                );
                f.render_widget(
                    Paragraph::new(feedback).style(Style::default().fg(
                        if app.notice.is_some() || form.harness_error.is_some() || app.mutation_busy
                        {
                            gold
                        } else {
                            muted
                        },
                    )),
                    rows[6],
                );
            }
            crate::app::Modal::ConfirmClose { .. }
            | crate::app::Modal::ConfirmGenerateEpic { .. } => {
                f.render_widget(
                    Paragraph::new(confirmation_lines.unwrap_or_default().join("\n"))
                        .style(Style::default().fg(ink)),
                    inner,
                );
            }
            crate::app::Modal::ConfirmDelete { id, title } => {
                let text = if app.mutation_busy {
                    format!("Deleting bead\nID: {id}\n\nCommand running · dialog locked")
                } else {
                    confirmation_lines
                        .unwrap_or_else(|| vec![format!("ID: {id}"), format!("Title: {title}")])
                        .join("\n")
                };
                f.render_widget(
                    Paragraph::new(text)
                        .block(Block::default())
                        .style(Style::default().fg(ink)),
                    inner,
                );
            }
            crate::app::Modal::ReviewResult {
                id,
                revision,
                accept,
                evidence,
            } => {
                let action = if *accept {
                    "Accept this exact result"
                } else {
                    "Request changes for this result"
                };
                let status = if app.mutation_busy {
                    "Recording review…"
                } else {
                    "Type criteria evidence or a reason. Enter submits. Esc cancels."
                };
                f.render_widget(Paragraph::new(format!("{action}\nWorktree: {id}\nCommit: {revision}\n\nEvidence: {evidence}\n\n{status}\nAcceptance does not merge, close or clean up.")).wrap(Wrap{trim:false}).style(Style::default().fg(ink)),inner);
            }
            crate::app::Modal::ConfirmWorktree { id, action } => {
                let action = match action {
                    0 => {
                        "Request merge resolution from the recorded agent? This is pending, not merged."
                    }
                    1 => "Close the integrated Herdr workspace?",
                    2 => {
                        "Remove integrated checkout and close its Herdr workspace? Branch is preserved."
                    }
                    5 => {
                        "Discard all uncommitted checkout changes?\n\nThe Herdr workspace will close and its\ncheckout will be removed without merging.\nThe branch is kept. The Bead stays open.\nNo replacement agent will start."
                    }
                    4 => {
                        "Ask the original idle agent to continue in this checkout? Existing changes are preserved. Verify its work before merging."
                    }
                    _ => {
                        "Restart this task from scratch?\n\nDiscard all uncommitted checkout changes.\nThe current Herdr workspace will close.\nThe old branch is kept. A fresh worktree\nand agent will start for this Bead."
                    }
                };
                f.render_widget(
                    Paragraph::new(format!(
                        "{action}\n\nWorktree: {id}\n\ny confirm · n / Esc cancel"
                    ))
                    .wrap(Wrap { trim: false })
                    .style(Style::default().fg(ink)),
                    inner,
                );
            }
            crate::app::Modal::Move {
                id,
                title,
                selected,
            } => {
                let statuses = crate::app::BOARD_STATUSES;
                let lines = std::iter::once(format!(
                    "{id} · {}",
                    truncate_display(title, inner.width as usize)
                ))
                .chain(statuses.iter().enumerate().map(|(index, status)| {
                    format!(
                        "{} {}",
                        if *selected == index { "›" } else { " " },
                        status_label(status)
                    )
                }))
                .chain(std::iter::once(
                    "↑/↓ choose · Enter move · Esc cancel".into(),
                ))
                .collect::<Vec<_>>();
                f.render_widget(
                    Paragraph::new(lines.into_iter().map(Line::from).collect::<Vec<_>>())
                        .style(Style::default().fg(ink)),
                    inner,
                );
            }
        }
    }
    render_toast(f, app);
    render_help(f, app);
}

fn dim_background(frame: &mut Frame, area: Rect) {
    for y in area.y..area.bottom() {
        for x in area.x..area.right() {
            if let Some(cell) = frame.buffer_mut().cell_mut((x, y)) {
                let style = cell.style();
                cell.set_style(Style {
                    fg: style.fg.map(dim_color),
                    bg: style.bg.map(dim_color),
                    ..style
                });
            }
        }
    }
}

fn dim_color(color: Color) -> Color {
    match color {
        Color::Rgb(r, g, b) => Color::Rgb(r / 2, g / 2, b / 2),
        Color::Reset => Color::Rgb(0, 0, 0),
        other => other,
    }
}

fn status_label(status: &str) -> &'static str {
    match status {
        "open" => "OPEN",
        "ready" => "READY",
        "blocked" => "BLOCKED",
        "in_progress" => "IN PROGRESS",
        "ready_to_merge" => "READY TO MERGE",
        "closed" => "CLOSED",
        _ => "OPEN",
    }
}

fn column_heading_parts(
    status: &str,
    focused: bool,
    count: usize,
    width: usize,
) -> (String, String) {
    let count = count.to_string();
    let count_width = Span::raw(count.as_str()).width();
    let label_width = width.saturating_sub(count_width + 1);
    let marker = if focused { "› " } else { "  " };
    let label = truncate_display(status_label(status), label_width.saturating_sub(2));
    (
        truncate_display(&format!("{marker}{label}"), label_width),
        count,
    )
}

fn board_columns(width: u16) -> u16 {
    if width >= 144 {
        6
    } else if width >= 54 {
        3
    } else {
        2
    }
}

fn render_kanban(f: &mut Frame, area: Rect, app: &mut App) {
    let statuses = crate::app::BOARD_STATUSES;
    let palette = crate::theme::definition(app.theme).palette;
    let columns = board_columns(area.width);
    let rows = 6 / columns;
    app.board_columns.clear();
    app.board_cards.clear();
    for (column, status) in statuses.iter().copied().enumerate() {
        let col = column as u16 % columns;
        let row = column as u16 / columns;
        let width = area.width / columns;
        let height = area.height / rows;
        let extra_width = area.width % columns;
        let extra_height = area.height % rows;
        let rect = Rect::new(
            area.x + col * width + col.min(extra_width),
            area.y + row * height + row.min(extra_height),
            width + u16::from(col < extra_width),
            height + u16::from(row < extra_height),
        );
        app.board_columns.push((column, rect));
        if rect.width < 2 || rect.height < 3 {
            continue;
        }
        let mut indices = crate::app::list_layout(&app.beads, &Default::default())
            .groups
            .into_iter()
            .find(|group| group.status == status)
            .map(|group| group.indices)
            .unwrap_or_default();
        indices.retain(|&i| crate::app::display_status(&app.beads[i]) == status);
        let card_height = 5usize;
        let can_show_cards = rect.height.saturating_sub(3) >= card_height as u16;
        let visible_cards = if can_show_cards {
            rect.height.saturating_sub(3) as usize / card_height
        } else {
            0
        };
        let mut offset = if can_show_cards {
            app.column_offsets[column].min(indices.len().saturating_sub(visible_cards))
        } else {
            0
        };
        if can_show_cards
            && app.focused_column == column
            && let Some(row) = app
                .selected
                .and_then(|selected| indices.iter().position(|&index| index == selected))
        {
            if row < offset {
                offset = row;
            } else if row >= offset + visible_cards {
                offset = row + 1 - visible_cards;
            }
        }
        app.column_offsets[column] = offset;
        let border = crate::theme::status_color(app.theme, status);
        f.render_widget(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(border))
                .style(Style::default().bg(palette.panel)),
            rect,
        );
        let (mut heading, count_label) = column_heading_parts(
            status,
            app.focused_column == column,
            indices.len(),
            rect.width.saturating_sub(2) as usize,
        );
        if app.focused_column == column {
            heading = heading.trim_start().to_owned();
            heading.insert(0, '›');
        }
        let count_width = Span::raw(count_label.as_str())
            .width()
            .min(rect.width.saturating_sub(2) as usize) as u16;
        let label_width = rect.width.saturating_sub(2 + count_width + 1);
        f.render_widget(
            Paragraph::new(heading).style(
                Style::default()
                    .fg(if app.focused_column == column {
                        palette.accent
                    } else {
                        border
                    })
                    .add_modifier(if app.focused_column == column {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    }),
            ),
            Rect::new(rect.x + 1, rect.y + 1, label_width, 1),
        );
        f.render_widget(
            Paragraph::new(count_label)
                .alignment(Alignment::Right)
                .style(
                    Style::default()
                        .fg(palette.foreground)
                        .add_modifier(Modifier::BOLD),
                ),
            Rect::new(
                rect.right().saturating_sub(1 + count_width),
                rect.y + 1,
                count_width,
                1,
            ),
        );
        let inner = Rect::new(
            rect.x + 1,
            rect.y + 2,
            rect.width.saturating_sub(2),
            rect.height.saturating_sub(3),
        );
        if indices.is_empty() {
            f.render_widget(
                Paragraph::new("No Beads").style(Style::default().fg(palette.muted)),
                inner,
            );
        }
        if !can_show_cards {
            app.column_offsets[column] = 0;
            f.render_widget(
                Paragraph::new(truncate_display(
                    &format!("Resize to {} rows to show cards", rows * 8 + 3),
                    inner.width as usize,
                ))
                .style(Style::default().fg(palette.muted)),
                inner,
            );
            continue;
        }
        for (n, &index) in indices.iter().enumerate().skip(offset).take(visible_cards) {
            let y = inner.y + ((n - offset) * card_height) as u16;
            if y + card_height as u16 > inner.y + inner.height {
                break;
            }
            let card = Rect::new(inner.x, y, inner.width, card_height as u16);
            app.board_cards.push((index, card));
            let bead = &app.beads[index];
            let selected = app.selected == Some(index);
            let style = if selected {
                Style::default()
                    .fg(palette.selected_foreground)
                    .bg(palette.selected_background)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(palette.foreground).bg(palette.panel)
            };
            let kind = row_issue_type(bead);
            let title_width = card.width.saturating_sub(2) as usize;
            let lines = vec![
                Line::from(truncate_display(&bead.title, title_width)),
                Line::from(vec![
                    Span::styled(
                        truncate_display(&bead.id, title_width),
                        Style::default().fg(if selected {
                            palette.selected_foreground
                        } else {
                            palette.accent
                        }),
                    ),
                    Span::raw(" "),
                    Span::styled(
                        truncate_display(&kind, 8),
                        Style::default().fg(crate::theme::issue_type_color(app.theme, &kind)),
                    ),
                    Span::raw(format!(" P{}", bead.priority)),
                ]),
                Line::from(Span::styled(
                    if !bead.blocked_by.is_empty() {
                        format!("WAIT:{}", bead.blocked_by.join(","))
                    } else if bead.status == "open"
                        && (app.accepted_handoffs.contains(&bead.id)
                            || app
                                .worktrees
                                .iter()
                                .any(|item| item.bead_id == bead.id && item.handoff_accepted))
                    {
                        "SENT".to_owned()
                    } else {
                        status_label(status).to_owned()
                    },
                    Style::default().fg(if bead.blocked_by.is_empty() {
                        border
                    } else {
                        palette.warning
                    }),
                )),
            ];
            f.render_widget(
                Paragraph::new(lines)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .border_type(BorderType::Rounded)
                            .border_style(
                                Style::default()
                                    .fg(crate::theme::issue_type_color(app.theme, &kind)),
                            )
                            .style(Style::default().bg(palette.panel)),
                    )
                    .style(style),
                card,
            );
        }
    }
}

fn render_drawer(f: &mut Frame, screen: Rect, app: &mut App, now: Instant) {
    let closing = app.drawer_animation.as_ref().filter(|animation| {
        app.settings.animate_drawers && animation.to == 0.0 && animation.fraction(now) > 0.0
    });
    if app.focus_detail || closing.is_some_and(|animation| !animation.worktrees) {
        render_detail_drawer(f, screen, app, now);
    } else if app.worktrees_view || closing.is_some_and(|animation| animation.worktrees) {
        render_worktree_drawer(f, screen, app, now);
    }
}

fn drawer_width(screen_width: u16, app: &App, now: Instant) -> u16 {
    let percent = match board_columns(screen_width) {
        6 => 40,
        3 => 70,
        _ => 100,
    };
    let target = screen_width.min((u32::from(screen_width) * percent / 100).max(36) as u16);
    let fraction = app
        .drawer_animation
        .as_ref()
        .filter(|_| app.settings.animate_drawers)
        .map_or(1.0, |animation| animation.fraction(now));
    (f32::from(target) * fraction).round() as u16
}

fn render_detail_drawer(f: &mut Frame, screen: Rect, app: &mut App, now: Instant) {
    let palette = crate::theme::definition(app.theme).palette;
    let width = drawer_width(screen.width, app, now);
    let area = Rect::new(
        screen.x + screen.width.saturating_sub(width),
        screen.y + 2,
        width,
        screen.height.saturating_sub(3),
    );
    app.detail_rect = area;
    if width < 4 {
        return;
    }
    f.render_widget(Clear, area);
    let selected = app.selected.and_then(|i| app.beads.get(i));
    let title = selected
        .map(|b| {
            format!(
                " BEAD DETAILS · {} · {} ",
                b.id,
                status_label(crate::app::display_status(b))
            )
        })
        .unwrap_or_else(|| " BEAD DETAILS ".into());
    let data = app.detail.as_ref().or_else(|| selected.map(|b| &b.data));
    let mut text = data
        .map(|d| {
            crate::detail::render(
                d,
                selected.map(|b| b.title.as_str()),
                area.width.saturating_sub(4),
            )
        })
        .unwrap_or_else(|| ratatui::text::Text::from("Select a Bead to inspect."));
    if let Some(bead) = selected {
        let metadata = vec![
            Line::from(format!("ID: {}", bead.id)),
            Line::from(format!(
                "Board column: {}",
                status_label(crate::app::display_status(bead))
            )),
            Line::from(format!("Raw status: {}", bead.status)),
            Line::from(format!("Priority: P{}", bead.priority)),
            Line::default(),
        ];
        text.lines.splice(0..0, metadata);
    }
    for line in &mut text.lines {
        for span in &mut line.spans {
            let old = span.style;
            let code = old.fg == Some(Color::Rgb(239, 183, 99))
                && !old.add_modifier.contains(Modifier::BOLD);
            let foreground = old
                .fg
                .map(|color| crate::theme::markdown_foreground(app.theme, color))
                .unwrap_or(palette.foreground);
            let background = old
                .bg
                .map(|color| crate::theme::markdown_background(app.theme, color))
                .unwrap_or(if code {
                    palette.code_background
                } else {
                    palette.background
                });
            span.style = old
                .fg(foreground)
                .bg(if code { palette.panel } else { background });
        }
    }
    let content_width = area.width.saturating_sub(2);
    let content_height = area.height.saturating_sub(2);
    let line_count = Paragraph::new(text.clone())
        .wrap(Wrap { trim: false })
        .line_count(content_width);
    app.detail_offset = app.detail_offset.min(
        line_count
            .saturating_sub(content_height as usize)
            .min(u16::MAX as usize) as u16,
    );
    f.render_widget(
        Paragraph::new(text)
            .block(
                Block::default()
                    .title(title)
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(palette.accent)),
            )
            .wrap(Wrap { trim: false })
            .scroll((app.detail_offset, 0))
            .style(
                Style::default()
                    .fg(palette.foreground)
                    .bg(palette.background),
            ),
        area,
    );
}

fn render_worktree_drawer(f: &mut Frame, screen: Rect, app: &mut App, now: Instant) {
    let palette = crate::theme::definition(app.theme).palette;
    let width = drawer_width(screen.width, app, now);
    let area = Rect::new(
        screen.x + screen.width.saturating_sub(width),
        screen.y + 2,
        width,
        screen.height.saturating_sub(3),
    );
    if width < 4 {
        app.list_rect = Rect::default();
        app.detail_rect = area;
        return;
    }
    f.render_widget(Clear, area);
    f.render_widget(
        Block::default()
            .title(" WORKTREES · GIT HANDOFFS · ESC CLOSES ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(palette.accent))
            .style(Style::default().bg(palette.panel)),
        area,
    );
    let inner = Rect::new(
        area.x + 1,
        area.y + 1,
        area.width.saturating_sub(2),
        area.height.saturating_sub(4),
    );
    let list_height = (inner.height / 3).max(5).min(inner.height);
    let list = Rect::new(inner.x, inner.y, inner.width, list_height);
    let detail = Rect::new(
        inner.x,
        inner.y + list_height,
        inner.width,
        inner.height.saturating_sub(list_height),
    );
    app.list_rect = list;
    app.detail_rect = detail;

    let visible = usize::from(list.height.saturating_sub(2) / 2).max(1);
    if app.selected_worktree < app.worktree_offset {
        app.worktree_offset = app.selected_worktree;
    }
    if app.selected_worktree >= app.worktree_offset + visible {
        app.worktree_offset = app.selected_worktree + 1 - visible;
    }
    app.worktree_offset = app
        .worktree_offset
        .min(app.worktrees.len().saturating_sub(visible));
    let rows = app
        .worktrees
        .iter()
        .enumerate()
        .skip(app.worktree_offset)
        .take(visible)
        .flat_map(|(i, item)| {
            let focused = i == app.selected_worktree;
            let status = crate::app::worktree_status_label(item.status);
            let style = if focused {
                Style::default()
                    .fg(palette.selected_foreground)
                    .bg(palette.selected_background)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(palette.foreground)
            };
            [
                Line::from(vec![
                    Span::styled(if focused { "› " } else { "  " }, style),
                    Span::styled(item.bead_id.clone(), style),
                    Span::raw(" · "),
                    Span::styled(
                        status,
                        Style::default()
                            .fg(crate::theme::worktree_status_color(app.theme, item.status))
                            .add_modifier(Modifier::BOLD),
                    ),
                ]),
                Line::from(Span::styled(
                    truncate_display(
                        &format!("  {} → {}", item.branch, item.target_branch),
                        inner.width as usize,
                    ),
                    Style::default().fg(palette.muted),
                )),
            ]
        })
        .collect::<Vec<_>>();
    let rows = if rows.is_empty() && !app.worktrees_loading {
        vec![Line::from("No outstanding tracked handoffs.")]
    } else {
        rows
    };
    f.render_widget(
        Paragraph::new(rows)
            .block(
                Block::default()
                    .title(" HANDOFFS ")
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(palette.border)),
            )
            .style(Style::default().bg(palette.panel)),
        list,
    );

    let selected = app.worktrees.get(app.selected_worktree);
    let mut lines = if let Some(item) = selected {
        let live = app.beads.iter().find(|bead| bead.id == item.bead_id);
        let title = live
            .map(|bead| bead.title.as_str())
            .or(item.bead_title.as_deref())
            .filter(|title| !title.trim().is_empty())
            .unwrap_or("(not recorded at handoff)");
        let description = live
            .and_then(|bead| bead.data.get("description"))
            .and_then(serde_json::Value::as_str)
            .or(item.bead_description.as_deref());
        let mut lines = vec![Line::from(vec![
            Span::styled(
                item.bead_id.clone(),
                Style::default()
                    .fg(palette.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" · "),
            Span::styled(
                crate::app::worktree_status_label(item.status),
                Style::default()
                    .fg(crate::theme::worktree_status_color(app.theme, item.status))
                    .add_modifier(Modifier::BOLD),
            ),
        ])];
        if let Some(detail) = &item.detail {
            lines.push(Line::from(Span::styled(
                detail.clone(),
                Style::default().fg(palette.warning),
            )));
        }
        lines.extend([
            Line::from(truncate_display(title, inner.width as usize)),
            Line::from(format!("Branch: {}", item.branch)),
            Line::from(format!("Target: {}", item.target_branch)),
            Line::from(format!("Workspace: {}", item.workspace_id)),
        ]);
        lines.push(Line::default());
        lines.push(Line::from("Description:"));
        lines.extend(
            description
                .unwrap_or("(none recorded at handoff)")
                .lines()
                .map(Line::from),
        );
        lines
    } else {
        vec![Line::from("Select a handoff to inspect its state.")]
    };
    let viewport = detail.height.saturating_sub(2);
    let count = Paragraph::new(lines.clone())
        .wrap(Wrap { trim: false })
        .line_count(detail.width.saturating_sub(2));
    app.worktree_detail_offset = app.worktree_detail_offset.min(
        count
            .saturating_sub(viewport as usize)
            .min(u16::MAX as usize) as u16,
    );
    f.render_widget(
        Paragraph::new(std::mem::take(&mut lines))
            .block(
                Block::default()
                    .title(" HANDOFF DETAILS ")
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(palette.border)),
            )
            .wrap(Wrap { trim: false })
            .scroll((app.worktree_detail_offset, 0))
            .style(Style::default().fg(palette.foreground).bg(palette.panel)),
        detail,
    );
    let footer = Rect::new(
        area.x + 2,
        area.bottom().saturating_sub(3),
        area.width.saturating_sub(4),
        2,
    );
    f.render_widget(
        Paragraph::new(vec![
            Line::from(truncate_display(
                "Enter switch · m merge · c close",
                footer.width as usize,
            )),
            Line::from(truncate_display(
                "x remove · X force · a continue · R restart",
                footer.width as usize,
            )),
        ])
        .style(Style::default().fg(palette.muted).bg(palette.panel_alt)),
        footer,
    );
}

fn render_planning(f: &mut Frame, area: Rect, form: &PlanningForm, theme: Theme, ready: bool) {
    let palette = crate::theme::definition(theme).palette;
    let block = Block::default()
        .title(" PLAN NEW WORK ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(palette.border))
        .style(Style::default().fg(palette.foreground).bg(palette.panel));
    f.render_widget(block, area);
    let inner = Rect::new(
        area.x.saturating_add(2),
        area.y.saturating_add(1),
        area.width.saturating_sub(4),
        area.height.saturating_sub(2),
    );
    if !ready {
        f.render_widget(
            Paragraph::new("Expand terminal to at least 50 columns and 12 rows · Esc cancels")
                .style(Style::default().fg(palette.warning)),
            inner,
        );
        return;
    }
    let (text, hint) = match &form.stage {
        PlanningStage::Draft => (
            format!("Describe the work to plan:\n\n{}▌", form.proposal),
            "Enter discuss with agent · Shift+Enter newline · Esc cancel",
        ),
        PlanningStage::Waiting => (
            format!(
                "Waiting for the planning agent…\n\nProposal:\n{}\n\nHerdr tab: {}",
                form.proposal,
                form.session
                    .as_ref()
                    .map_or("starting", |session| &session.tab_id)
            ),
            "Esc leaves the planning tab open for review",
        ),
        PlanningStage::Question { text, options } => {
            let mut display = format!("Agent asks:\n\n{text}\n\n");
            for (index, option) in options.iter().enumerate() {
                display.push_str(&format!(
                    "{} {}. {}\n",
                    if form.selected == index { '›' } else { ' ' },
                    index + 1,
                    option
                ));
            }
            display.push_str(&format!(
                "{} Custom answer: {}{}",
                if form.selected == options.len() {
                    '›'
                } else {
                    ' '
                },
                form.input,
                if form.selected == options.len() {
                    "▌"
                } else {
                    ""
                }
            ));
            (
                display,
                "↑/↓ choose · Tab custom answer · Enter send · Esc cancel",
            )
        }
        PlanningStage::Review(plan) => {
            let mut display = format!(
                "Proposed {} · no Beads created yet\n\n{}  P{}\n{}\n\nAcceptance:\n{}",
                plan.kind.to_uppercase(),
                plan.root.title,
                plan.root.priority,
                plan.root.description,
                plan.root.acceptance
            );
            for (index, task) in plan.tasks.iter().enumerate() {
                display.push_str(&format!(
                    "\n\n{}. {}  P{}\n{}\n\nAcceptance:\n{}",
                    index + 1,
                    task.title,
                    task.priority,
                    task.description,
                    task.acceptance
                ));
            }
            (
                display,
                "y create Beads · r request revision · ↑/↓ scroll · Esc cancel",
            )
        }
        PlanningStage::Revision => (
            format!("Tell the agent what to revise:\n\n{}▌", form.input),
            "Enter send revision · Shift+Enter newline · Esc cancel",
        ),
        PlanningStage::Creating => (
            "Creating the confirmed Beads…\n\nThis may take a moment.".into(),
            "Creation in progress",
        ),
        PlanningStage::Failed(error) => (
            format!("Planning needs attention:\n\n{error}"),
            "r read the agent again (if available) · Esc close",
        ),
    };
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);
    let scroll = if matches!(form.stage, PlanningStage::Draft | PlanningStage::Revision)
        || matches!(&form.stage, PlanningStage::Question { options, .. } if form.selected == options.len())
    {
        wrap_words(&text, sections[0].width as usize)
            .len()
            .saturating_sub(sections[0].height as usize)
            .min(u16::MAX as usize) as u16
    } else {
        form.scroll
    };
    f.render_widget(
        Paragraph::new(text)
            .wrap(Wrap { trim: false })
            .scroll((scroll, 0))
            .style(Style::default().fg(palette.foreground)),
        sections[0],
    );
    f.render_widget(
        Paragraph::new(truncate_display(hint, sections[1].width as usize))
            .style(Style::default().fg(palette.accent)),
        sections[1],
    );
}
fn render_help(f: &mut Frame, app: &mut App) {
    let Some(help) = app.help.as_mut() else {
        return;
    };
    let screen = f.area();
    if screen.width < 18 || screen.height < 5 {
        f.render_widget(Clear, screen);
        f.render_widget(
            Paragraph::new("Expand terminal to view hotkeys · Esc closes")
                .alignment(Alignment::Center),
            screen,
        );
        return;
    }
    let palette = crate::theme::definition(app.theme).palette;
    let width = screen.width.saturating_sub(2).min(82);
    let height = screen.height.saturating_sub(2).min(30);
    let area = Rect::new(
        screen.x + (screen.width - width) / 2,
        screen.y + (screen.height - height) / 2,
        width,
        height,
    );
    let inner = Rect::new(
        area.x + 2,
        area.y + 2,
        area.width.saturating_sub(4),
        area.height.saturating_sub(5),
    );
    let key_width = help
        .context
        .entries()
        .iter()
        .map(|(key, _)| Span::raw(*key).width())
        .max()
        .unwrap_or(12)
        .min(20)
        .min((inner.width as usize).saturating_sub(6).max(1));
    let action_width = (inner.width as usize).saturating_sub(key_width + 2).max(1);
    let mut lines = Vec::new();
    for (key, action) in help.context.entries() {
        let keys = wrap_words(key, key_width);
        let actions = wrap_words(action, action_width);
        for row in 0..keys.len().max(actions.len()) {
            let label = keys.get(row).map(String::as_str).unwrap_or("");
            let padding = key_width.saturating_sub(Span::raw(label).width());
            let description = actions.get(row).map(String::as_str).unwrap_or("");
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{label}{}  ", " ".repeat(padding)),
                    Style::default().fg(palette.accent),
                ),
                Span::styled(
                    description.to_owned(),
                    Style::default().fg(palette.foreground),
                ),
            ]));
        }
    }
    let viewport = usize::from(inner.height).max(1);
    help.offset = help.offset.min(lines.len().saturating_sub(viewport));
    f.render_widget(Clear, area);
    f.render_widget(
        Block::default()
            .title(format!(" HOTKEYS · {} ", help.context.title()))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(palette.accent))
            .style(Style::default().bg(palette.panel)),
        area,
    );
    f.render_widget(
        Paragraph::new(lines).scroll((help.offset.min(u16::MAX as usize) as u16, 0)),
        inner,
    );
    let footer = if area.width < 45 {
        "Esc close · ↑↓ scroll"
    } else {
        "Esc / Shift+? close · ↑↓ / PgUp PgDn scroll"
    };
    f.render_widget(
        Paragraph::new(truncate_display(
            footer,
            area.width.saturating_sub(4) as usize,
        ))
        .style(Style::default().fg(palette.muted)),
        Rect::new(
            area.x + 2,
            area.y + area.height - 2,
            area.width.saturating_sub(4),
            1,
        ),
    );
}
fn render_toast(f: &mut Frame, app: &App) {
    let Some(toast) = app
        .toast
        .as_ref()
        .filter(|toast| toast.visible(std::time::Instant::now()))
    else {
        return;
    };
    let area = f.area();
    let width = area.width.saturating_sub(2).min(56);
    if width < 4 || area.height < 3 {
        return;
    }
    let text_width = usize::from(width.saturating_sub(4)).max(1);
    let mut lines = wrap_words(&toast.message, text_width);
    let height = (lines.len() as u16 + 2).min(area.height.saturating_sub(1));
    let visible_lines = usize::from(height.saturating_sub(2));
    if lines.len() > visible_lines && visible_lines > 0 {
        lines.truncate(visible_lines);
        lines[visible_lines - 1] = truncate_display("… press w for handoffs", text_width);
    }
    let colors = crate::theme::definition(app.theme).palette;
    let border = if toast.error {
        colors.danger
    } else {
        colors.accent
    };
    let box_area = Rect::new(area.x + area.width - width, area.y, width, height);
    f.render_widget(Clear, box_area);
    f.render_widget(
        Paragraph::new(lines.join("\n"))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .title(" HERDR ")
                    .border_style(Style::default().fg(border)),
            )
            .style(Style::default().fg(colors.foreground).bg(colors.panel)),
        box_area,
    );
}
fn footer_text(width: usize) -> String {
    truncate_display(
        "↑↓ select · ←→ column · ⇧←→ move · m move · Enter details · w worktrees · n new · e edit · q quit",
        width,
    )
}

fn bead_type_name(index: usize) -> &'static str {
    crate::beads::BEAD_TYPES
        .get(index)
        .copied()
        .unwrap_or("task")
}
fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|first| first.to_uppercase().collect::<String>() + c.as_str())
        .unwrap_or_default()
}
fn display_window(s: &str, desired_start: usize, width: usize) -> (String, usize) {
    let mut out = String::new();
    let mut cell = 0;
    let mut actual_start = 0;
    let mut visible = 0;
    for ch in s.chars() {
        let cw = Span::raw(ch.to_string()).width();
        if cell < desired_start && cell + cw > desired_start {
            actual_start = cell + cw;
        }
        if cell >= desired_start && visible + cw <= width {
            out.push(ch);
            visible += cw;
        }
        cell += cw;
        if cell <= desired_start {
            actual_start = cell;
        }
        if visible >= width {
            break;
        }
    }
    (out, actual_start)
}
pub(crate) const FORM_DESCRIPTION_WIDTH: usize = 64;

fn description_layout(
    s: &str,
    cursor: usize,
    width: usize,
) -> (String, usize, usize, Vec<(usize, usize, usize)>) {
    let width = width.max(1);
    let cursor = cursor.min(s.len());
    let mut caret = None;
    let mut positions = Vec::with_capacity(s.chars().count() + 1);
    let mut out = String::new();
    let mut row = 0;
    let mut col = 0;
    for (byte, ch) in s.char_indices() {
        if ch == '\n' {
            let position = if col == width {
                (0, row + 1)
            } else {
                (col, row)
            };
            positions.push((byte, position.0, position.1));
            if byte == cursor {
                caret = Some(position);
            }
            out.push('\n');
            row += 1;
            col = 0;
            continue;
        }
        if !ch.is_whitespace()
            && (byte == 0
                || s[..byte]
                    .chars()
                    .next_back()
                    .is_some_and(char::is_whitespace))
        {
            let word = s[byte..].split(char::is_whitespace).next().unwrap_or("");
            if col > 0 && col + Span::raw(word).width() > width {
                out.push('\n');
                row += 1;
                col = 0;
            }
        }
        // A separator at the right edge belongs to the next visual line, but
        // need not occupy a cell there before the next word.
        if ch == ' '
            && col == width
            && s[byte + ch.len_utf8()..]
                .chars()
                .next()
                .is_some_and(|next| !next.is_whitespace())
        {
            out.push('\n');
            row += 1;
            col = 0;
            positions.push((byte, col, row));
            if byte == cursor {
                caret = Some((col, row));
            }
            continue;
        }
        let cw = Span::raw(ch.to_string()).width();
        if col == width || col + cw > width {
            out.push('\n');
            row += 1;
            col = 0;
        }
        positions.push((byte, col, row));
        if byte == cursor {
            caret = Some((col, row));
        }
        out.push(ch);
        col += cw;
    }
    let end = if col == width {
        (0, row + 1)
    } else {
        (col, row)
    };
    positions.push((s.len(), end.0, end.1));
    if cursor == s.len() {
        caret = Some(end);
    }
    let (x, y) = caret.unwrap_or((col, row));
    (out, x, y, positions)
}

fn wrapped_description(s: &str, cursor: usize, width: usize) -> (String, usize, usize) {
    let (text, x, y, _) = description_layout(s, cursor, width);
    (text, x, y)
}

pub(crate) fn move_description_vertical(
    s: &str,
    cursor: usize,
    down: bool,
    preferred_column: &mut Option<usize>,
) -> usize {
    let (_, x, y, positions) = description_layout(s, cursor, FORM_DESCRIPTION_WIDTH);
    let column = *preferred_column.get_or_insert(x);
    let target_row = if down { y + 1 } else { y.saturating_sub(1) };
    if target_row == y {
        return cursor;
    }
    positions
        .into_iter()
        .filter(|(_, _, row)| *row == target_row)
        .min_by_key(|(byte, x, _)| (x.abs_diff(column), std::cmp::Reverse(*byte)))
        .map_or(cursor, |(byte, _, _)| byte)
}
fn wrapped_lines(s: &str, width: usize) -> Vec<String> {
    wrapped_description(s, 0, width)
        .0
        .split('\n')
        .map(str::to_owned)
        .collect()
}
fn wrap_words(s: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut lines = Vec::new();
    for paragraph in s.lines() {
        let mut line = String::new();
        for word in paragraph.split_whitespace() {
            if Span::raw(word).width() > width {
                if !line.is_empty() {
                    lines.push(std::mem::take(&mut line));
                }
                lines.extend(wrapped_lines(word, width));
            } else if line.is_empty() {
                line.push_str(word);
            } else if Span::raw(&line).width() + 1 + Span::raw(word).width() <= width {
                line.push(' ');
                line.push_str(word);
            } else {
                lines.push(std::mem::take(&mut line));
                line.push_str(word);
            }
        }
        if !line.is_empty() || paragraph.is_empty() {
            lines.push(line);
        }
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}
fn truncate_display(s: &str, width: usize) -> String {
    if Span::raw(s).width() <= width {
        return s.to_owned();
    }
    if width == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut used = 0;
    for ch in s.chars() {
        let cw = Span::raw(ch.to_string()).width();
        if used + cw + 1 > width {
            break;
        }
        out.push(ch);
        used += cw;
    }
    out.push('…');
    out
}
fn delete_dialog_lines(
    id: &str,
    title: &str,
    width: usize,
    error: Option<&str>,
    busy: bool,
) -> Vec<String> {
    if busy {
        return vec![
            "Deleting bead".into(),
            format!("ID: {id}"),
            "Command running · dialog locked".into(),
        ];
    }
    let mut lines = vec!["Permanently delete this bead?".into()];
    lines.extend(wrapped_lines(&format!("ID: {id}"), width));
    lines.push(format!(
        "Title: {}",
        truncate_display(title, width.saturating_sub(7))
    ));
    lines.push("Cannot be undone.".into());
    lines.extend(wrapped_lines(
        "Dependency links are removed; dependents may be orphaned.",
        width,
    ));
    lines.extend(wrapped_lines(
        "No cascade: other beads will not be deleted.",
        width,
    ));
    if let Some(error) = error {
        lines.push(truncate_display(&format!("Delete error: {error}"), width));
    }
    lines.push("[y] Delete · [n / Enter] Cancel".into());
    lines
}
#[cfg(test)]
fn type_color(index: usize) -> Color {
    [
        Color::Rgb(73, 211, 182),
        Color::Rgb(255, 123, 123),
        Color::Rgb(103, 177, 255),
        Color::Rgb(197, 145, 255),
        Color::Rgb(178, 192, 196),
        Color::Rgb(255, 192, 115),
        Color::Rgb(119, 204, 219),
        Color::Rgb(240, 141, 197),
        Color::Rgb(168, 207, 125),
    ][index.min(8)]
}
#[cfg(test)]
fn priority_color(index: usize) -> Color {
    [
        Color::Rgb(255, 105, 105),
        Color::Rgb(255, 159, 105),
        Color::Rgb(239, 183, 99),
        Color::Rgb(138, 197, 169),
        Color::Rgb(123, 144, 148),
    ][index.min(4)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::beads::Bead;
    use ratatui::{Terminal, backend::TestBackend};
    use serde_json::json;
    use std::sync::mpsc;

    fn app() -> App {
        let (tx, _) = mpsc::channel();
        let (_, responses) = mpsc::channel();
        App {
            beads: vec![Bead {
                dependency_ready: true,
                blocked_by: Vec::new(),
                id: "demo-1".into(),
                title: "Title is visible".into(),
                status: "closed".into(),
                priority: 1,
                data: json!({"issue_type":"task","unknown":{"n":5},"comments":[{"body":"hello comment"}],"unicode":"界界界界界界界界界界"}),
            }],
            focused_column: 5,
            column_offsets: [0; 6],
            board_columns: Vec::new(),
            board_cards: Vec::new(),
            column_rows: [0; 6],
            selected: Some(0),
            detail: None,
            focus_detail: false,
            detail_offset: 0,
            drawer_animation: None,
            list_loading: false,
            detail_loading: false,
            list_error: None,
            detail_error: None,
            detail_seq: 0,
            edit_seq: 0,
            harness_seq: 0,
            launch_seq: 0,
            handoff: Default::default(),
            handoff_capacity: Default::default(),
            toast: None,
            list_rect: Rect::default(),
            detail_rect: Rect::default(),
            modal: None,
            help: None,
            settings: crate::settings::Settings::default(),
            theme: crate::settings::Theme::Dark,
            settings_path: None,
            settings_load_error: None,
            worktrees: Vec::new(),
            accepted_handoffs: std::collections::HashSet::new(),
            known_handoffs: std::collections::HashSet::new(),
            scheduler_attempted: std::collections::HashSet::new(),
            last_handoff_failure: None,
            selected_worktree: 0,
            worktree_offset: 0,
            worktree_detail_offset: 0,
            worktrees_view: false,
            worktrees_loading: false,
            worktrees_error: None,
            worktrees_focus_feedback: None,
            worktrees_focus_pending: false,
            worktrees_inflight: false,
            worktrees_discovered: true,
            worktrees_refresh_pending: false,
            worktree_action_pending: false,
            worktrees_next_refresh: std::time::Instant::now(),
            agent_observation_inflight: false,
            next_agent_observation: std::time::Instant::now(),
            notice: None,
            mutation_busy: false,
            ready_pending: false,
            planning_epics: Default::default(),
            easter_egg: false,
            easter_egg_progress: 0,
            modal_ready: true,
            paste_seq: 0,
            paste_pending: false,
            created_screenshots: Vec::new(),
            preferred_id: None,
            refresh_inflight: false,
            refresh_pending: false,
            next_refresh: std::time::Instant::now(),
            requests: tx,
            responses,
        }
    }
    fn draw(size: (u16, u16), a: &mut App) -> (String, Rect) {
        let mut t = Terminal::new(TestBackend::new(size.0, size.1)).unwrap();
        t.draw(|f| render(f, Duration::from_secs(2), a)).unwrap();
        (
            t.backend()
                .buffer()
                .content()
                .iter()
                .map(|c| c.symbol())
                .collect(),
            a.list_rect,
        )
    }
    fn draw_at(size: (u16, u16), elapsed: Duration, app: &mut App) -> String {
        let mut terminal = Terminal::new(TestBackend::new(size.0, size.1)).unwrap();
        terminal.draw(|frame| render(frame, elapsed, app)).unwrap();
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect()
    }
    fn draw_drawer(size: (u16, u16), app: &mut App, now: Instant) -> String {
        let mut terminal = Terminal::new(TestBackend::new(size.0, size.1)).unwrap();
        terminal
            .draw(|frame| {
                let screen = frame.area();
                render_drawer(frame, screen, app, now);
            })
            .unwrap();
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect()
    }
    #[test]
    fn bordered_screens_render_without_square_corners() {
        let mut app = app();
        for screen in ["issues", "worktrees", "form", "help"] {
            app.worktrees_view = screen == "worktrees";
            app.modal = (screen == "form")
                .then(|| crate::app::Modal::Create(crate::app::CreateForm::default()));
            app.help = (screen == "help").then_some(crate::hotkeys::Help {
                context: crate::hotkeys::Context::Form,
                offset: 0,
            });
            let rendered = draw((100, 30), &mut app).0;
            assert!(rendered.contains('╭'), "{screen} has no rounded border");
            assert!(
                !rendered
                    .chars()
                    .any(|corner| matches!(corner, '┌' | '┐' | '└' | '┘')),
                "{screen} contains a square corner"
            );
        }
    }
    #[test]
    fn shared_modal_dims_background_but_not_modal_and_restores_after_close() {
        for theme in [crate::settings::Theme::Dark, crate::settings::Theme::Light] {
            for settings in [false, true] {
                let mut app = app();
                app.theme = theme;
                app.focus_detail = true;
                let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
                terminal
                    .draw(|f| render(f, Duration::from_secs(2), &mut app))
                    .unwrap();
                let normal = terminal.backend().buffer().content().to_vec();
                let backdrop_index = 2 + 4 * 100;
                let background = &normal[backdrop_index];
                let drawer_index = 85 + 8 * 100;
                let drawer_before = &normal[drawer_index];

                app.modal = Some(if settings {
                    crate::app::Modal::Settings {
                        draft: app.settings,
                        error: None,
                        dropdown_open: false,
                        focus: 0,
                    }
                } else {
                    crate::app::Modal::Create(crate::app::CreateForm::default())
                });
                terminal
                    .draw(|f| render(f, Duration::from_secs(2), &mut app))
                    .unwrap();
                let buffer = terminal.backend().buffer();
                let backdrop = &buffer.content()[backdrop_index];
                assert_eq!(backdrop.symbol(), background.symbol());
                assert_eq!(
                    backdrop.fg,
                    dim_color(background.fg),
                    "{theme:?} foreground"
                );
                assert_eq!(
                    backdrop.bg,
                    dim_color(background.bg),
                    "{theme:?} background"
                );
                let drawer_after = &buffer.content()[drawer_index];
                assert_eq!(
                    drawer_after.bg,
                    dim_color(drawer_before.bg),
                    "{theme:?} drawer background"
                );
                assert_eq!(
                    drawer_after.fg,
                    dim_color(drawer_before.fg),
                    "{theme:?} drawer foreground"
                );

                assert!(
                    buffer.content().iter().any(|cell| {
                        cell.symbol() == "╭"
                            && cell.fg == crate::theme::definition(theme).palette.border
                    }),
                    "{theme:?} modal border should retain its themed color"
                );
                assert!(
                    buffer.content().iter().any(|cell| {
                        cell.symbol() != " "
                            && cell.bg == crate::theme::definition(theme).palette.panel
                            && cell.fg == crate::theme::definition(theme).palette.foreground
                    }),
                    "{theme:?} dialog text should retain its themed style"
                );

                app.modal = None;
                terminal
                    .draw(|f| render(f, Duration::from_secs(2), &mut app))
                    .unwrap();
                assert_eq!(terminal.backend().buffer().content(), normal.as_slice());
            }
        }
    }
    #[test]
    fn help_toast_and_compact_modal_fallback_keep_their_normal_styles() {
        for theme in [crate::settings::Theme::Dark, crate::settings::Theme::Light] {
            let mut app = app();
            app.theme = theme;
            let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
            terminal
                .draw(|f| render(f, Duration::from_secs(2), &mut app))
                .unwrap();
            let background = terminal.backend().buffer()[(2, 4)].clone();
            app.help = Some(crate::hotkeys::Help {
                context: crate::hotkeys::Context::Issues,
                offset: 0,
            });
            app.toast = Some(crate::app::Toast {
                message: "launching".into(),
                error: false,
                expires_at: None,
            });
            terminal
                .draw(|f| render(f, Duration::from_secs(2), &mut app))
                .unwrap();
            assert_eq!(terminal.backend().buffer()[(2, 4)].bg, background.bg);
            assert_eq!(terminal.backend().buffer()[(2, 4)].fg, background.fg);

            app.modal = Some(crate::app::Modal::Create(crate::app::CreateForm::default()));
            let mut tiny = Terminal::new(TestBackend::new(20, 6)).unwrap();
            tiny.draw(|f| render(f, Duration::from_secs(2), &mut app))
                .unwrap();
            let cells = tiny.backend().buffer().content();
            assert!(
                cells
                    .iter()
                    .any(|cell| cell.fg == crate::theme::definition(theme).palette.muted)
            );
        }
    }
    #[test]
    fn splash_animates_girl_without_moving_her_or_delaying_browser() {
        let mut app = app();
        let idle = draw_at((100, 30), Duration::ZERO, &mut app);
        let wave = draw_at((100, 30), Duration::from_millis(300), &mut app);
        let blink = draw_at((100, 30), Duration::from_millis(600), &mut app);
        assert!(idle.contains("███████"));
        assert!(idle.contains("o   o"));
        assert!(wave.contains("o/"));
        assert!(blink.contains("-   -"));
        assert_eq!(
            idle.find(".-\"\"\"-."),
            wave.find(".-\"\"\"-."),
            "the character should stay centered between poses"
        );
        assert_eq!(idle.find(".-\"\"\"-."), blink.find(".-\"\"\"-."));
        let browser = draw_at((100, 30), SPLASH_DURATION, &mut app);
        assert!(browser.contains("Title is visible"));
        assert!(!browser.contains(".-\"\"\"-."));
    }
    #[test]
    fn splash_uses_character_and_title_when_compact_and_text_when_tiny() {
        let mut app = app();
        let compact = draw_at((60, 18), Duration::from_millis(300), &mut app);
        assert!(compact.contains("ONTRACK"));
        assert!(compact.contains(".-\"\"\"-."));
        assert!(!compact.contains("███████"));
        let tiny = draw_at((24, 7), Duration::from_millis(300), &mut app);
        assert!(tiny.contains("ONTRACK"));
        assert!(!tiny.contains(".-\"\"\"-."));
        app.help = Some(crate::hotkeys::Help {
            context: crate::hotkeys::Context::Issues,
            offset: 0,
        });
        let help = draw_at((60, 18), Duration::ZERO, &mut app);
        assert!(help.contains("HOTKEYS"));
        assert!(!help.contains(".-\"\"\"-."));
    }
    #[test]
    fn easter_egg_is_centered_themed_and_responsive() {
        assert_eq!(EGG_ART.len(), 51);
        for (row, line) in EGG_ART.iter().enumerate() {
            assert_eq!(Span::raw(*line).width(), 65, "art row {row}");
        }
        for theme in [Theme::Dark, Theme::Light] {
            let mut app = app();
            app.theme = theme;
            let palette = crate::theme::definition(theme).palette;
            // Populate browser hit targets before entering the exclusive screen.
            draw((100, 60), &mut app);
            assert!(!app.board_cards.is_empty());
            app.easter_egg = true;
            app.help = Some(crate::hotkeys::Help {
                context: crate::hotkeys::Context::Issues,
                offset: 0,
            });
            app.toast = Some(crate::app::Toast {
                message: "hidden toast".into(),
                error: false,
                expires_at: None,
            });
            for (width, height) in [(100, 60), (101, 60), (65, 53)] {
                let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
                terminal
                    .draw(|f| render(f, Duration::ZERO, &mut app))
                    .unwrap();
                let buffer = terminal.backend().buffer();
                let left = width / 2 - 65 / 2;
                let top = (height - 53) / 2;
                let hint = "Esc to return";
                let hint_left = width / 2 - Span::raw(hint).width() as u16 / 2;
                for y in 0..height {
                    let expected = if (top..top + 51).contains(&y) {
                        format!(
                            "{}{}{}",
                            " ".repeat(left as usize),
                            EGG_ART[(y - top) as usize],
                            " ".repeat((width - left - 65) as usize)
                        )
                    } else if y == top + 52 {
                        format!(
                            "{}{}{}",
                            " ".repeat(hint_left as usize),
                            hint,
                            " ".repeat((width - hint_left - 13) as usize)
                        )
                    } else {
                        " ".repeat(width as usize)
                    };
                    let actual: String = (0..width).map(|x| buffer[(x, y)].symbol()).collect();
                    assert_eq!(actual, expected, "{theme:?} {width}x{height} row {y}");
                    for x in 0..width {
                        let cell = &buffer[(x, y)];
                        assert_eq!(cell.bg, palette.background);
                        if (top..top + 51).contains(&y) && (left..left + 65).contains(&x) {
                            assert_eq!(cell.fg, palette.accent);
                        } else if y == top + 52 && (hint_left..hint_left + 13).contains(&x) {
                            assert_eq!(cell.fg, palette.muted);
                        }
                    }
                }
                assert_eq!(app.list_rect, Rect::default());
                assert_eq!(app.detail_rect, Rect::default());
                assert!(app.board_columns.is_empty());
                assert!(app.board_cards.is_empty());
                assert!(!app.modal_ready);
            }
            for (width, height) in [(64, 53), (65, 52), (30, 10), (3, 1)] {
                let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
                terminal
                    .draw(|f| render(f, Duration::ZERO, &mut app))
                    .unwrap();
                let buffer = terminal.backend().buffer();
                let left = width / 2 - 1;
                let top = (height - 1) / 2;
                for y in 0..height {
                    let expected = if y == top {
                        format!(
                            "{}(o){}",
                            " ".repeat(left as usize),
                            " ".repeat((width - left - 3) as usize)
                        )
                    } else {
                        " ".repeat(width as usize)
                    };
                    let actual: String = (0..width).map(|x| buffer[(x, y)].symbol()).collect();
                    assert_eq!(actual, expected, "{theme:?} {width}x{height} row {y}");
                    for x in 0..width {
                        assert_eq!(buffer[(x, y)].bg, palette.background);
                        if y == top && (left..left + 3).contains(&x) {
                            assert_eq!(buffer[(x, y)].fg, palette.accent);
                        }
                    }
                }
            }
            for size in [(4, 2), (1, 1), (0, 0)] {
                let output = draw_at(size, Duration::ZERO, &mut app);
                assert!(!output.contains("Title is visible"));
                assert!(!output.contains("HOTKEYS"));
                assert!(!output.contains("hidden toast"));
            }
        }
    }
    #[test]
    fn hotkey_reference_has_columns_and_scrolls_in_compact_terminals() {
        let mut a = app();
        a.help = Some(crate::hotkeys::Help {
            context: crate::hotkeys::Context::Form,
            offset: 0,
        });
        let (wide, _) = draw((80, 15), &mut a);
        assert!(wide.contains("HOTKEYS · Create / edit form"));
        assert!(wide.contains("Esc / Shift+? close"));
        assert!(wide.contains("Text / Space"));
        assert!(wide.contains("Type in title or description"));
        a.help.as_mut().unwrap().offset = usize::MAX;
        let (compact, _) = draw((30, 10), &mut a);
        assert!(a.help.unwrap().offset > 0);
        assert!(compact.contains("HOTKEYS"));
        assert!(compact.contains("Esc"));
    }

    #[test]
    fn ready_issue_shows_waiting_bead_ids_beside_status_and_clears_on_refresh() {
        let mut a = app();
        a.focused_column = 1;
        a.beads[0].status = "open".into();
        a.beads[0].data["labels"] = json!([crate::beads::READY_LABEL]);
        a.beads[0].dependency_ready = false;
        a.beads[0].blocked_by = vec!["base-1".into(), "base-2".into()];
        let screen = draw((150, 30), &mut a).0;
        assert!(screen.contains("WAIT:base-1,base-2"), "{screen}");
        // Compact terminals must keep rendering and navigation functional.
        draw((40, 12), &mut a);
        a.beads[0].dependency_ready = true;
        a.beads[0].blocked_by.clear();
        assert!(!draw((150, 30), &mut a).0.contains("waiting on"));
    }

    #[test]
    fn open_issue_shows_confirmed_agent_handoff_only() {
        let mut a = app();
        a.focused_column = 0;
        a.beads[0].status = "open".into();
        a.beads[0].data["issue_type"] = json!("task");
        a.worktrees.push(crate::worktrees::ManagedWorktree {
            id: "wt-1".into(),
            bead_id: "demo-1".into(),
            parent_bead_id: None,
            handoff_accepted: false,
            bead_title: None,
            bead_description: None,
            branch: "ontrack/demo-1".into(),
            target_branch: "main".into(),
            workspace_id: "ws".into(),
            status: crate::worktrees::WorktreeStatus::NeedsAttention,
            detail: None,
        });
        assert!(!draw((80, 24), &mut a).0.contains("· sent"));
        a.worktrees[0].handoff_accepted = true;
        assert!(draw((80, 24), &mut a).0.contains("SENT"));
        a.worktrees.clear();
        a.accepted_handoffs.insert("demo-1".into());
        assert!(draw((80, 24), &mut a).0.contains("SENT"));
        a.beads[0].status = "in_progress".into();
        a.focused_column = 3;
        assert!(draw((80, 24), &mut a).0.contains("IN PROGRESS"));
        assert!(!draw((80, 24), &mut a).0.contains("· sent"));
    }

    #[test]
    fn markdown_description_is_readable_and_styled_in_detail_pane() {
        let mut a = app();
        a.focus_detail = true;
        a.detail = Some(json!({"description":"# Overview\n\nA **bold** point and `code`."}));
        let mut terminal = Terminal::new(TestBackend::new(100, 24)).unwrap();
        terminal
            .draw(|f| render(f, Duration::from_secs(2), &mut a))
            .unwrap();
        let buffer = terminal.backend().buffer();
        let cells = buffer.content();
        assert!(
            !cells
                .iter()
                .map(|cell| cell.symbol())
                .collect::<String>()
                .contains("**bold**")
        );
        let visible = cells.iter().map(|cell| cell.symbol()).collect::<String>();
        assert!(visible.contains("Overview"));
        assert!(
            cells
                .iter()
                .any(|cell| cell.symbol() == "b" && cell.modifier.contains(Modifier::BOLD))
        );
    }
    #[test]
    fn dependency_chart_priority_colors_follow_every_theme() {
        for theme in crate::settings::Theme::ALL {
            let mut a = app();
            a.focus_detail = true;
            a.theme = theme;
            a.detail = Some(json!({"id": "demo-1", "title": "Release", "priority": 2,
                "blocked_by": [{"id": "CRITICAL", "priority": 0}],
                "blocks": [{"id": "FOLLOWUP", "priority": 3}],
                "dependents": (0..8).map(|i| json!({"id": format!("child-{i}"),
                    "priority": i % 5, "status": "open", "dependency_type": "parent-child",
                    "title": "Decode the complete recording into an ordered replay timeline"
                })).collect::<Vec<_>>()
            }));
            let mut terminal = Terminal::new(TestBackend::new(180, 40)).unwrap();
            terminal
                .draw(|f| render(f, Duration::from_secs(2), &mut a))
                .unwrap();
            let buffer = terminal.backend().buffer();
            for (name, priority) in [("CRITICAL", 0), ("FOLLOWUP", 3)] {
                let row = (0..40)
                    .find(|y| {
                        (0..180)
                            .map(|x| buffer[(x, *y)].symbol())
                            .collect::<String>()
                            .contains(name)
                    })
                    .unwrap();
                let cell = (a.list_rect.width..180)
                    .find(|x| buffer[(*x, row)].symbol() == &name[..1])
                    .unwrap();
                assert_eq!(
                    buffer[(cell, row)].fg,
                    crate::theme::priority_color(theme, priority)
                );
            }
        }
    }

    #[test]
    fn selected_card_border_contrasts_with_fill_in_every_theme() {
        for theme in crate::settings::Theme::ALL {
            let mut a = app();
            let mut unselected = a.beads[0].clone();
            unselected.id = "demo-2".into();
            unselected.title = "Unselected card".into();
            unselected.data["issue_type"] = serde_json::Value::String("feature".into());
            a.beads.push(unselected);
            a.theme = theme;
            let mut terminal = Terminal::new(TestBackend::new(180, 40)).unwrap();
            terminal
                .draw(|f| render(f, Duration::from_secs(2), &mut a))
                .unwrap();
            let buffer = terminal.backend().buffer();
            let card = a
                .board_cards
                .iter()
                .find(|(index, _)| *index == 0)
                .unwrap()
                .1;
            let palette = crate::theme::definition(theme).palette;
            for (x, y, glyph) in (card.x..card.right())
                .flat_map(|x| {
                    [
                        (
                            x,
                            card.y,
                            if x == card.x {
                                "╭"
                            } else if x + 1 == card.right() {
                                "╮"
                            } else {
                                "─"
                            },
                        ),
                        (
                            x,
                            card.bottom() - 1,
                            if x == card.x {
                                "╰"
                            } else if x + 1 == card.right() {
                                "╯"
                            } else {
                                "─"
                            },
                        ),
                    ]
                })
                .chain(
                    (card.y + 1..card.bottom() - 1)
                        .flat_map(|y| [(card.x, y, "│"), (card.right() - 1, y, "│")]),
                )
            {
                let cell = &buffer[(x, y)];
                assert_eq!(cell.symbol(), glyph);
                assert_eq!(cell.fg, crate::theme::issue_type_color(theme, "task"));
                assert_eq!(cell.bg, palette.panel);
                assert_ne!(cell.fg, cell.bg);
            }
            let unselected_card = a
                .board_cards
                .iter()
                .find(|(index, _)| *index == 1)
                .unwrap()
                .1;
            assert_eq!(unselected_card.width, card.width);
            assert_eq!(unselected_card.height, card.height);
            for (x, y) in (unselected_card.x..unselected_card.right())
                .flat_map(|x| [(x, unselected_card.y), (x, unselected_card.bottom() - 1)])
                .chain(
                    (unselected_card.y + 1..unselected_card.bottom() - 1)
                        .flat_map(|y| [(unselected_card.x, y), (unselected_card.right() - 1, y)]),
                )
            {
                let cell = &buffer[(x, y)];
                assert_ne!(cell.symbol(), " ");
                assert_eq!(cell.fg, crate::theme::issue_type_color(theme, "feature"));
            }
            let title_x = card.x + 1;
            let title_y = card.y + 1;
            let title_cell = &buffer[(title_x, title_y)];
            assert_eq!(title_cell.bg, palette.selected_background);
            assert_eq!(title_cell.fg, palette.selected_foreground);
            let selected_position = a
                .board_cards
                .iter()
                .find(|(index, _)| *index == 0)
                .unwrap()
                .1;
            assert_eq!(selected_position, card);
            let rendered = buffer
                .content()
                .iter()
                .map(|cell| cell.symbol())
                .collect::<String>();
            assert!(rendered.contains("Title is visible"));
            assert!(rendered.contains("demo-1"));
        }
    }

    #[test]
    fn card_border_uses_legacy_and_fallback_issue_types() {
        for theme in crate::settings::Theme::ALL {
            for (field, value, expected) in [
                ("issue_type", None, "unknown"),
                ("type", Some("bug"), "bug"),
                ("issue_type", Some("custom"), "custom"),
            ] {
                let mut a = app();
                if field == "type" {
                    a.beads[0]
                        .data
                        .as_object_mut()
                        .unwrap()
                        .remove("issue_type");
                }
                if let Some(value) = value {
                    a.beads[0].data[field] = serde_json::Value::String(value.into());
                } else {
                    a.beads[0].data.as_object_mut().unwrap().remove(field);
                }
                a.theme = theme;
                let mut terminal = Terminal::new(TestBackend::new(180, 40)).unwrap();
                terminal
                    .draw(|f| render(f, Duration::from_secs(2), &mut a))
                    .unwrap();
                let card = a.board_cards[0].1;
                let cell = &terminal.backend().buffer()[(card.x, card.y)];
                assert_eq!(cell.symbol(), "╭");
                assert_eq!(cell.fg, crate::theme::issue_type_color(theme, expected));
                assert_eq!(cell.bg, crate::theme::definition(theme).palette.panel);
            }
        }
    }

    #[test]
    fn dependency_chart_renders_and_scrolls_after_resize() {
        let mut a = app();
        a.focus_detail = true;
        a.detail = Some(json!({
            "id": "demo-1", "title": "Ship", "status": "open",
            "dependencies": [{"id": "base", "dependency_type": "blocks", "title": "Build", "status": "closed"}],
            "dependents": [{"id": "next", "dependency_type": "blocks", "title": "FINAL_NODE"}]
        }));
        let (wide, _) = draw((120, 40), &mut a);
        for expected in [
            "DEPENDENCY FLOW",
            "Blocked by ← prerequisites",
            "Blocks → dependents",
            "base [closed]",
            "FINAL_NODE",
        ] {
            assert!(wide.contains(expected), "missing {expected}: {wide}");
        }
        for size in [(50, 14), (80, 18), (120, 24)] {
            a.detail_offset = u16::MAX;
            let (scrolled, _) = draw(size, &mut a);
            if size.0 == 50 {
                assert!(a.detail_offset > 0);
            }
            assert!(scrolled.contains("FINAL_NODE"), "{scrolled}");
        }
        a.detail_offset = 0;
        let (top, _) = draw((120, 40), &mut a);
        assert!(top.contains("DEPENDENCY FLOW"));
        assert!(top.contains("Blocked by ← prerequisites"));
    }

    #[test]
    fn issue_details_hide_fallback_sections_at_all_scroll_positions() {
        let mut a = app();
        a.focus_detail = true;
        let detail = json!({
            "id": "demo-1", "title": "Ship",
            "description": "Useful description",
            "notes": "Useful notes", "design": "Useful design",
            "acceptance_criteria": "Useful acceptance",
            "comments": [{"body": "Useful comment"}],
            "owner": "HIDDEN_OWNER", "custom": {"nested": "HIDDEN_CUSTOM"},
            "dependencies": [
                {"id": "base", "type": "blocks", "title": "Build", "extra": "HIDDEN_EXTRA"},
                {"unsupported": "HIDDEN_RECORD"}
            ]
        });
        a.detail = Some(detail.clone());
        for size in [(50, 14), (120, 40)] {
            let mut visible = String::new();
            for offset in 0..40 {
                a.detail_offset = offset;
                let (screen, _) = draw(size, &mut a);
                for omitted in [
                    "RELATIONSHIP DETAILS",
                    "OTHER DETAILS",
                    "HIDDEN_OWNER",
                    "HIDDEN_CUSTOM",
                    "HIDDEN_EXTRA",
                    "HIDDEN_RECORD",
                ] {
                    assert!(!screen.contains(omitted), "{screen}");
                }
                visible.push_str(&screen);
            }
            for expected in [
                "Useful description",
                "Useful notes",
                "Useful design",
                "Useful acceptance",
                "Useful comment",
                "base",
                "Build",
            ] {
                assert!(visible.contains(expected), "missing {expected}: {visible}");
            }
        }
        assert_eq!(a.detail.as_ref(), Some(&detail));
    }

    #[test]
    fn tiny_terminal_startup_and_wrapped_detail_are_safe() {
        let mut a = app();
        a.list_rect = Rect::new(1, 1, 10, 10);
        assert!(!draw((18, 5), &mut a).0.is_empty());
        assert_eq!(a.list_rect, Rect::default());
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal
            .draw(|f| render(f, Duration::ZERO, &mut a))
            .unwrap();
        assert_eq!(a.list_rect, Rect::default());
        a.list_loading = true;
        let (s, _) = draw((80, 24), &mut a);
        assert!(!s.contains("REFRESHING"));
        a.list_loading = false;
        a.detail_error = Some("broken".into());
        let (s, _) = draw((80, 24), &mut a);
        assert!(s.contains("DETAIL ERROR"));
        a.beads.clear();
        a.selected = None;
        a.detail = None;
        a.detail_error = None;
        assert!(draw((80, 24), &mut a).0.contains("No Beads"));
        a.beads.push(Bead {
            dependency_ready: true,
            blocked_by: Vec::new(),
            id: "long".into(),
            title: "long".into(),
            status: "open".into(),
            priority: 2,
            data: json!({"description":"界".repeat(700)}),
        });
        a.selected = Some(0);
        a.beads[0].data = json!({"description": format!("{}FINAL_SENTINEL", "word\n".repeat(800))});
        a.detail = Some(a.beads[0].data.clone());
        a.focus_detail = true;
        a.detail_offset = u16::MAX;
        let (rendered, _) = draw((50, 14), &mut a);
        assert!(a.detail_offset > 0);
        assert!(rendered.contains("FINAL_SENTINEL"));
        assert!(a.detail_rect.height > 0);
        let (resized, _) = draw((90, 24), &mut a);
        assert!(resized.contains("FINAL_SENTINEL"));
    }
    #[test]
    fn close_confirmation_renders_target_errors_and_locks_small_terminals() {
        let mut a = app();
        a.modal = Some(crate::app::Modal::ConfirmClose {
            id: "test-1".into(),
            title: "Close selected task 界".into(),
        });
        for size in [(100, 30), (50, 24), (100, 16)] {
            let screen = draw(size, &mut a).0;
            for text in [
                "CLOSE BEAD?",
                "ID: test-1",
                "Close selected task 界",
                "Verified Git integration is required first",
                "[y] Close",
                "Esc] Cancel",
            ] {
                assert!(screen.contains(text), "missing {text} at {size:?}");
            }
            assert!(a.modal_ready);
        }
        a.notice = Some("Close failed: blocked by task".into());
        assert!(draw((100, 30), &mut a).0.contains("blocked by task"));
        a.mutation_busy = true;
        assert!(draw((100, 30), &mut a).0.contains("Closing bead test-1"));
        a.mutation_busy = false;
        for size in [(40, 24), (100, 8)] {
            assert!(draw(size, &mut a).0.contains("Expand terminal"));
            assert!(!a.modal_ready);
        }
    }

    #[test]
    fn detail_metadata_preserves_raw_status_and_priority_without_relationships() {
        for size in [(50, 24), (120, 30)] {
            let mut a = app();
            a.beads[0].status = "deferred".into();
            a.beads[0].priority = 3;
            a.beads[0].data = json!({"issue_type":"task","description":"No relationships here"});
            a.focus_detail = true;
            let screen = draw(size, &mut a).0;
            for text in [
                "ID: demo-1",
                "Board column: OPEN",
                "Raw status: deferred",
                "Priority: P3",
            ] {
                assert!(
                    screen.contains(text),
                    "missing {text} at {size:?}: {screen}"
                );
            }
            assert!(!screen.contains("Raw status: open"));
        }
    }

    #[test]
    fn focused_empty_column_has_non_color_marker_and_compact_counts_stay_visible() {
        let mut a = app();
        a.beads.clear();
        a.focused_column = 5;
        let mut terminal = Terminal::new(TestBackend::new(120, 20)).unwrap();
        terminal
            .draw(|frame| render(frame, Duration::from_secs(2), &mut a))
            .unwrap();
        let column = a
            .board_columns
            .iter()
            .find(|(index, _)| *index == 5)
            .unwrap()
            .1;
        let buffer = terminal.backend().buffer();
        let heading = (column.x..column.right())
            .map(|x| buffer[(x, column.y + 1)].symbol())
            .collect::<String>();
        assert!(heading.contains('›'), "{heading}");
        assert!(heading.contains('0'), "{heading}");

        a.beads = (0..123).map(|i| Bead {
            dependency_ready: true,
            blocked_by: Vec::new(),
            id: format!("c{i}"),
            title: format!("Card {i}"),
            status: "in_progress".into(),
            priority: 2,
            data: json!({"issue_type":"task","status":"in_progress","labels":[crate::beads::READY_TO_MERGE_LABEL]}),
        }).collect();
        a.focused_column = 4;
        let screen = draw((120, 20), &mut a).0;
        assert!(screen.contains("123"), "{screen}");
        assert!(screen.contains("123"));
    }

    #[test]
    fn short_board_heights_show_resize_guidance_without_stale_hit_rectangles() {
        let mut a = app();
        a.beads[0].status = "open".into();
        a.focused_column = 0;
        for height in [7, 8, 9] {
            a.board_cards.push((99, Rect::new(1, 1, 1, 1)));
            let screen = draw((144, height), &mut a).0;
            assert!(
                screen.contains("Resize to 11 rows"),
                "height {height}: {screen}"
            );
            assert!(a.board_cards.is_empty(), "stale cards at height {height}");
        }
        let screen = draw((144, 10), &mut a).0;
        assert!(screen.contains("Resize to 11 rows"), "{screen}");
        assert!(a.board_cards.is_empty());
        let screen = draw((144, 12), &mut a).0;
        assert!(screen.contains("Title is visible"), "{screen}");
        assert!(
            a.board_cards
                .iter()
                .any(|(index, rect)| *index == 0 && rect.height > 0)
        );
    }

    #[test]
    fn create_and_delete_modals_render_at_normal_and_tiny_sizes() {
        let mut a = app();
        a.notice = Some("Created previous".into());
        a.list_error = Some("current failure".into());
        let status = draw((120, 40), &mut a).0;
        assert!(status.contains("LIST ERROR"));
        a.list_error = None;
        a.notice = None;
        let status = draw((120, 40), &mut a).0;
        assert!(status.contains("n new"));
        assert!(status.contains("e edit"));
        assert!(status.contains("n new"));
        a.modal = Some(crate::app::Modal::Create(crate::app::CreateForm::default()));
        let normal = draw((100, 30), &mut a).0;
        assert!(normal.contains("NEW BEAD"));
        assert!(normal.contains("Task"));
        assert!(normal.contains("P2"));
        assert!(!normal.contains("Title · required"));
        a.modal = Some(crate::app::Modal::Create(crate::app::CreateForm {
            title: "Visible title".into(),
            cursor: 13,
            ..Default::default()
        }));
        let entered = draw((100, 30), &mut a).0;
        assert!(entered.contains("Visible title"));
        assert!(!entered.contains("Title · required"));
        a.modal = Some(crate::app::Modal::Edit {
            id: "demo-1".into(),
            form: crate::app::CreateForm {
                title: "Edited title".into(),
                cursor: 12,
                description: "Existing description".into(),
                ..Default::default()
            },
        });
        let edit = draw((100, 30), &mut a).0;
        assert!(edit.contains("EDIT BEAD"));
        assert!(edit.contains("Edited title"));
        assert!(edit.contains("Existing description"));
        assert!(edit.contains("[ Save ]"));
        a.modal = Some(crate::app::Modal::EditLoading {
            id: "demo-1".into(),
            seq: 1,
            error: Some("offline".into()),
        });
        let loading_error = draw((100, 30), &mut a).0;
        assert!(loading_error.contains("offline"));
        assert!(loading_error.contains("retry"));
        a.modal = Some(crate::app::Modal::Create(crate::app::CreateForm::default()));
        let tiny = draw((12, 5), &mut a).0;
        assert!(tiny.contains("Resize"));
        assert!(!a.modal_ready);
        a.modal = Some(crate::app::Modal::ConfirmDelete {
            id: "demo-1".into(),
            title: "Title is visible".into(),
        });
        let deletion = draw((100, 30), &mut a).0;
        assert!(deletion.contains("Cannot be undone"));
        assert!(deletion.contains("orphaned"));
        let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
        a.modal = Some(crate::app::Modal::Create(crate::app::CreateForm {
            focus: 2,
            dropdown: Some(2),
            issue_type: 8,
            ..Default::default()
        }));
        terminal
            .draw(|f| render(f, Duration::ZERO, &mut a))
            .unwrap();
        let buf = terminal.backend().buffer();
        let text = buf.content().iter().map(|c| c.symbol()).collect::<String>();
        assert!(text.contains("Milestone"));
        assert!(text.contains("NEW BEAD"));
        assert!(a.modal_ready);
        assert!(buf.content().iter().any(|cell| {
            cell.bg
                == crate::theme::definition(a.theme)
                    .palette
                    .selected_background
        }));
        assert_eq!(
            buf[(17, 9)].fg,
            crate::theme::definition(a.theme).palette.accent
        );
        a.notice = None;
        a.modal = Some(crate::app::Modal::Create(crate::app::CreateForm {
            focus: 5,
            ..Default::default()
        }));
        let normal_80 = draw((80, 24), &mut a).0;
        assert!(normal_80.contains("Cancel"));
        assert!(normal_80.contains("Create"));
        a.modal = Some(crate::app::Modal::ConfirmDelete {
            id: "demo-1".into(),
            title: "Title is visible".into(),
        });
        let delete_80 = draw((80, 24), &mut a).0;
        assert!(delete_80.contains("[y] Delete"));
        assert!(delete_80.contains("orphaned"));
        a.modal = Some(crate::app::Modal::ConfirmDelete {
            id: "long-id".into(),
            title: "A very long title ".repeat(12),
        });
        let long_delete = draw((50, 12), &mut a).0;
        assert!(a.modal_ready);
        assert!(long_delete.contains("long-id"));
        assert!(long_delete.contains("Cannot be undone"));
        assert!(long_delete.contains("Dependency links"));
        assert!(long_delete.contains("No cascade"));
        assert!(long_delete.contains("[y] Delete"));
        assert!(long_delete.contains("…"));
        a.notice = Some("permission denied".into());
        let error_delete = draw((80, 24), &mut a).0;
        assert!(error_delete.contains("Delete error"));
        assert!(error_delete.contains("permission denied"));
        assert!(error_delete.contains("[y] Delete"));
    }
    #[test]
    fn planning_controls_are_absent_from_epic_screen_and_create_form() {
        let mut app = app();
        app.selected = Some(0);
        app.beads[0].data["issue_type"] = serde_json::json!("epic");
        let screen = draw((130, 30), &mut app).0;
        assert!(!screen.contains("g generate"));
        assert!(!screen.contains("p plan"));
        for focus_detail in [false, true] {
            app.focus_detail = focus_detail;
            assert!(
                !crate::hotkeys::Context::current(&app)
                    .entries()
                    .iter()
                    .any(|(key, _)| *key == "g" || *key == "p")
            );
        }
        app.modal = Some(crate::app::Modal::Create(crate::app::CreateForm {
            issue_type: crate::beads::BEAD_TYPES
                .iter()
                .position(|kind| *kind == "epic")
                .unwrap(),
            ..Default::default()
        }));
        let form = draw((100, 30), &mut app).0;
        assert!(!form.contains("Generate child tasks"));
        assert!(form.contains("Agent harness"));
        assert!(form.contains("[ Create ]"));
    }

    #[test]
    fn create_harness_picker_shows_only_discovered_kinds_and_discovery_states() {
        let mut app = app();
        app.modal = Some(crate::app::Modal::Create(crate::app::CreateForm {
            harnesses: vec!["opencode".into()],
            harness: 1,
            focus: 6,
            dropdown: Some(6),
            ..Default::default()
        }));
        let choices = draw((100, 30), &mut app).0;
        assert!(choices.contains("opencode"));
        assert!(choices.contains("Unassigned"));
        assert!(!choices.contains("claude"));
        app.modal = Some(crate::app::Modal::Create(crate::app::CreateForm {
            harness_loading: true,
            ..Default::default()
        }));
        assert!(
            draw((100, 30), &mut app)
                .0
                .contains("Checking Herdr integrations")
        );
        app.modal = Some(crate::app::Modal::Create(crate::app::CreateForm {
            harness_error: Some("Herdr integration discovery failed: offline".into()),
            ..Default::default()
        }));
        let failed = draw((100, 30), &mut app).0;
        assert!(failed.contains("Herdr unavailable"));
        assert!(failed.contains("offline"));
    }

    #[test]
    fn restart_dialog_shows_destructive_warning_and_handoff_id() {
        let mut app = app();
        app.worktrees_view = true;
        app.modal = Some(crate::app::Modal::ConfirmWorktree {
            id: "ot-selected".into(),
            action: 3,
        });
        let rendered = draw((100, 30), &mut app).0;
        assert!(rendered.contains("RESTART TASK?"));
        assert!(rendered.contains("Discard all uncommitted checkout changes."));
        assert!(rendered.contains("Worktree: ot-selected"));
        assert!(rendered.contains("y confirm"));
    }
    #[test]
    fn force_remove_dialog_shows_data_loss_warning_and_handoff_id() {
        let mut app = app();
        app.worktrees_view = true;
        app.modal = Some(crate::app::Modal::ConfirmWorktree {
            id: "ot-selected".into(),
            action: 5,
        });
        let rendered = draw((100, 30), &mut app).0;
        assert!(rendered.contains("FORCE REMOVE WORKTREE?"));
        assert!(rendered.contains("Discard all uncommitted checkout changes?"));
        assert!(rendered.contains("Worktree: ot-selected"));
        assert!(rendered.contains("y confirm"));
        assert!(rendered.contains("The branch is kept. The Bead stays open."));
    }
    #[test]
    fn continue_dialog_names_existing_checkout_and_keeps_confirmation_visible() {
        let mut app = app();
        app.worktrees_view = true;
        app.modal = Some(crate::app::Modal::ConfirmWorktree {
            id: "ot-selected".into(),
            action: 4,
        });
        let rendered = draw((100, 30), &mut app).0;
        assert!(rendered.contains("CONTINUE TASK?"));
        assert!(rendered.contains("Existing changes are preserved"));
        assert!(rendered.contains("y confirm"));
    }
    #[test]
    fn board_footer_documents_navigation_and_drawers_without_overflow() {
        for width in [40, 64, 90, 120] {
            let footer = footer_text(width);
            assert!(Span::raw(footer.as_str()).width() <= width);
            assert!(footer.contains("↑↓"));
        }
        let footer = footer_text(140);
        for action in ["←→ column", "move", "Enter details", "worktrees", "n new"] {
            assert!(footer.contains(action));
        }
        let mut app = app();
        let original = draw((120, 24), &mut app).0;
        app.list_error = Some("current failure".into());
        app.notice = Some("Updated bead".into());
        let with_feedback = draw((120, 24), &mut app).0;
        assert_eq!(
            original.chars().skip(120 * 23).collect::<String>(),
            with_feedback.chars().skip(120 * 23).collect::<String>()
        );
        assert!(with_feedback.contains("LIST ERROR · current failure"));
        app.list_error = None;
        app.detail_error = Some("detail unavailable".into());
        let with_detail_error = draw((120, 24), &mut app).0;
        assert_eq!(
            original.chars().skip(120 * 23).collect::<String>(),
            with_detail_error.chars().skip(120 * 23).collect::<String>()
        );
        assert!(with_detail_error.contains("DETAIL ERROR · detail unavailable"));
        app.detail_error = None;
        let with_notice = draw((120, 24), &mut app).0;
        assert_eq!(
            original.chars().skip(120 * 23).collect::<String>(),
            with_notice.chars().skip(120 * 23).collect::<String>()
        );
        assert!(with_notice.contains("Updated bead"));
    }

    #[test]
    fn outstanding_worktrees_render_target_state_and_pending_dispatch_on_compact_terminal() {
        let mut app = app();
        app.worktrees_view = true;
        app.worktrees.push(crate::worktrees::ManagedWorktree {
            id: "handoff-1".into(),
            bead_id: "closed-1".into(),
            parent_bead_id: None,
            handoff_accepted: true,
            bead_title: Some("Review handoff".into()),
            bead_description: Some("First line\nSecond line".into()),
            branch: "ontrack/closed-1".into(),
            target_branch: "main".into(),
            workspace_id: "ws-7".into(),
            status: crate::worktrees::WorktreeStatus::MergeRequested,
            detail: Some("waiting for verified ancestry".into()),
        });
        let screen = draw((80, 16), &mut app).0;
        assert!(screen.contains("WORKTREES"));
        assert!(screen.contains("closed-1"));
        assert!(screen.contains("MERGE PENDING"));
        assert!(screen.contains("main"));
        assert!(screen.contains("waiting for verified ancestry"));
        assert!(screen.contains("GIT HANDOFFS"));
        for action in ["Enter switch", "m merge", "c close", "x remove"] {
            assert!(screen.contains(action));
        }
        app.worktrees_error = Some("scan failed".into());
        let with_error = draw((80, 16), &mut app).0;
        assert!(with_error.contains("WORKTREE ERROR · scan failed"));
        app.worktrees_focus_feedback = Some(crate::app::WorktreeFocusFeedback::Success(
            "Switched workspace".into(),
        ));
        app.worktrees_error = None;
        let with_focus = draw((80, 16), &mut app).0;
        assert!(with_focus.contains("Switched workspace"));
        assert!(app.detail_rect.height > 0);
    }

    #[test]
    fn worktree_status_text_uses_theme_colours_in_list_and_detail() {
        use crate::worktrees::{ManagedWorktree, WorktreeStatus};
        assert_eq!(
            crate::app::worktree_status_label(WorktreeStatus::Unmerged),
            "READY TO MERGE"
        );
        let statuses = [
            WorktreeStatus::Reserved,
            WorktreeStatus::InProgress,
            WorktreeStatus::Unmerged,
            WorktreeStatus::MergeQueued,
            WorktreeStatus::MergeRequested,
            WorktreeStatus::MergeInProgress,
            WorktreeStatus::NeedsResolution,
            WorktreeStatus::Integrated,
            WorktreeStatus::WorkspaceClosed,
            WorktreeStatus::CheckoutRemoved,
            WorktreeStatus::DirtyCheckout,
            WorktreeStatus::NeedsAttention,
        ];
        let locate = |buffer: &ratatui::buffer::Buffer, rect: Rect, label: &str| {
            let chars = label.chars().collect::<Vec<_>>();
            for y in rect.y..rect.bottom() {
                for x in rect.x..rect.right().saturating_sub(chars.len() as u16) {
                    if chars.iter().enumerate().all(|(offset, ch)| {
                        buffer[(x + offset as u16, y)].symbol() == ch.to_string()
                    }) {
                        return (x, y);
                    }
                }
            }
            panic!("{label} missing from {rect:?}")
        };
        for theme in Theme::ALL {
            let mut app = app();
            app.theme = theme;
            app.beads[0].status = "open".into();
            app.focused_column = 0;
            app.worktrees_view = true;
            app.worktrees = statuses
                .iter()
                .enumerate()
                .map(|(index, &status)| ManagedWorktree {
                    id: format!("handoff-{index}"),
                    bead_id: format!("bead-{index}"),
                    parent_bead_id: None,
                    handoff_accepted: true,
                    bead_title: None,
                    bead_description: None,
                    branch: format!("ontrack/bead-{index}"),
                    target_branch: "main".into(),
                    workspace_id: format!("ws-{index}"),
                    status,
                    detail: None,
                })
                .collect();
            let mut terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();
            for (index, status) in statuses.into_iter().enumerate() {
                app.selected_worktree = index;
                terminal
                    .draw(|f| render(f, Duration::from_secs(2), &mut app))
                    .unwrap();
                let buffer = terminal.backend().buffer();
                let label = crate::app::worktree_status_label(status);
                let (x, y) = locate(buffer, app.detail_rect, label);
                for offset in 0..label.len() as u16 {
                    let cell = &buffer[(x + offset, y)];
                    assert_eq!(cell.fg, crate::theme::worktree_status_color(theme, status));
                    assert_eq!(cell.bg, crate::theme::definition(theme).palette.panel);
                    assert!(cell.modifier.contains(Modifier::BOLD));
                }
            }
        }
    }
    #[test]
    fn selected_worktree_shows_its_captured_title_and_multiline_description() {
        let mut app = app();
        app.worktrees_view = true;
        let first = crate::worktrees::ManagedWorktree {
            id: "handoff-1".into(),
            bead_id: "bead-1".into(),
            parent_bead_id: None,
            handoff_accepted: true,
            bead_title: Some("First task".into()),
            bead_description: Some("Line one\nLine two".into()),
            branch: "ontrack/bead-1".into(),
            target_branch: "main".into(),
            workspace_id: "ws-1".into(),
            status: crate::worktrees::WorktreeStatus::Unmerged,
            detail: None,
        };
        app.worktrees.push(first.clone());
        app.worktrees.push(crate::worktrees::ManagedWorktree {
            id: "handoff-2".into(),
            bead_id: "bead-2".into(),
            parent_bead_id: None,
            bead_title: Some("Second task".into()),
            bead_description: Some("Another description".into()),
            ..first
        });
        let first_screen = draw((100, 23), &mut app).0;
        for value in [
            "First task",
            "Line one",
            "Line two",
            "Branch: ontrack/bead-1",
        ] {
            assert!(first_screen.contains(value), "{value} missing");
        }
        app.selected_worktree = 1;
        let second_screen = draw((100, 23), &mut app).0;
        for value in ["Second task", "Another description"] {
            assert!(second_screen.contains(value), "{value} missing");
        }
        assert!(!second_screen.contains("Line one"));
    }
    #[test]
    fn worktree_description_scrolls_and_missing_context_has_fallback() {
        let mut app = app();
        app.worktrees_view = true;
        app.worktrees.push(crate::worktrees::ManagedWorktree {
            id: "legacy".into(),
            bead_id: "old-bead".into(),
            parent_bead_id: None,
            handoff_accepted: false,
            bead_title: None,
            bead_description: None,
            branch: "ontrack/old-bead".into(),
            target_branch: "main".into(),
            workspace_id: "ws".into(),
            status: crate::worktrees::WorktreeStatus::Unmerged,
            detail: None,
        });
        let legacy = draw((100, 20), &mut app).0;
        assert!(legacy.contains("old-bead"));
        assert!(legacy.contains("not recorded at handoff"));
        app.worktrees[0].bead_description = Some(
            (0..30)
                .map(|index| format!("Detail line {index}"))
                .collect::<Vec<_>>()
                .join("\n"),
        );
        app.worktree_detail_offset = u16::MAX;
        let scrolled = draw((100, 20), &mut app).0;
        assert!(app.worktree_detail_offset > 0);
        assert!(scrolled.contains("Detail line 29"));
    }
    #[test]
    fn current_bead_text_updates_worktree_details_and_snapshot_survives_bead_removal() {
        let mut app = app();
        app.worktrees_view = true;
        app.worktrees.push(crate::worktrees::ManagedWorktree {
            id: "handoff".into(),
            bead_id: "demo-1".into(),
            parent_bead_id: None,
            handoff_accepted: true,
            bead_title: Some("Original title".into()),
            bead_description: Some("Original description".into()),
            branch: "ontrack/demo-1".into(),
            target_branch: "main".into(),
            workspace_id: "ws".into(),
            status: crate::worktrees::WorktreeStatus::Unmerged,
            detail: None,
        });
        app.beads[0].title = "Updated title".into();
        app.beads[0].data["description"] = json!("Updated description");
        let current = draw((100, 23), &mut app).0;
        assert!(current.contains("Updated title"));
        assert!(current.contains("Updated description"));
        assert!(!current.contains("Original description"));
        app.beads.clear();
        let fallback = draw((100, 23), &mut app).0;
        assert!(fallback.contains("Original title"));
        assert!(fallback.contains("Original description"));
    }
    #[test]
    fn selected_worktree_and_confirmation_stay_visible_with_long_lists() {
        let mut app = app();
        app.worktrees_view = true;
        for index in 0..12 {
            app.worktrees.push(crate::worktrees::ManagedWorktree {
                id: format!("wt-{index}"),
                bead_id: format!("bead-{index}"),
                parent_bead_id: None,
                handoff_accepted: true,
                bead_title: None,
                bead_description: None,
                branch: format!("ontrack/bead-{index}"),
                target_branch: "main".into(),
                workspace_id: format!("ws-{index}"),
                status: crate::worktrees::WorktreeStatus::Unmerged,
                detail: None,
            });
        }
        app.selected_worktree = 11;
        app.modal = Some(crate::app::Modal::ConfirmWorktree {
            id: "wt-11".into(),
            action: 0,
        });
        let screen = draw((80, 9), &mut app).0;
        assert!(screen.contains("Worktree: wt-11"));
        assert!(screen.contains("Request merge"));
        assert!(screen.contains("recorded agent"));
        assert!(screen.contains("confirm"));
        assert!(app.worktree_offset > 0);
    }
    #[test]
    fn remove_confirmation_names_herdr_closure_and_keeps_consent_visible_compactly() {
        let mut state = app();
        state.worktrees_view = true;
        state.worktrees.push(crate::worktrees::ManagedWorktree {
            id: "wt-remove".into(),
            bead_id: "b-closed".into(),
            parent_bead_id: None,
            handoff_accepted: true,
            bead_title: None,
            bead_description: None,
            branch: "ontrack/b".into(),
            target_branch: "main".into(),
            workspace_id: "ws".into(),
            status: crate::worktrees::WorktreeStatus::Integrated,
            detail: None,
        });
        state.settings.auto_close_workspace = false;
        state.modal = Some(crate::app::Modal::ConfirmWorktree {
            id: "wt-remove".into(),
            action: 2,
        });
        let screen = draw((80, 9), &mut state).0;
        assert!(screen.contains("Remove integrated checkout"));
        assert!(screen.contains("Herdr workspace?"));
        assert!(screen.contains("y confirm · n / Esc cancel"));
        let mut settings = app();
        settings.modal = Some(crate::app::Modal::Settings {
            draft: crate::settings::Settings {
                auto_remove_checkout: true,
                ..Default::default()
            },
            error: None,
            dropdown_open: false,
            focus: 3,
        });
        let screen = draw((80, 24), &mut settings).0;
        assert!(screen.contains("Remove checkout + close Herdr workspace"));
    }
    #[test]
    fn wrapping_and_caret_share_unicode_line_map() {
        let (s, x, y) = wrapped_description("abcdefghij\nxy", 12, 4);
        assert_eq!(s, "abcd\nefgh\nij\nxy");
        assert_eq!((x, y), (1, 3));
        let (s, x, y) = wrapped_description("abc\n", 3, 3);
        assert_eq!(s, "abc\n");
        assert_eq!((x, y), (0, 1));
        let (_, x, y) = wrapped_description("abc\n", 4, 3);
        assert_eq!((x, y), (0, 1));
        let (s, x, y) = wrapped_description("ab界x", 5, 3);
        assert_eq!(s, "ab\n界x");
        assert_eq!((x, y), (2, 1));
        let (_, x, y) = wrapped_description("ab界x", 6, 3);
        assert_eq!((x, y), (0, 2));
        let (_, x, y) = wrapped_description("one\ntwo\nthree\nfour", 19, 5);
        let scroll = y.saturating_sub(2);
        assert_eq!(y - scroll, 2);
        assert!(x < 5);
    }
    #[test]
    fn description_wraps_words_and_keeps_cursor_on_the_rendered_line() {
        let (s, x, y) = wrapped_description("hello world", 6, 8);
        assert_eq!(s, "hello \nworld");
        assert_eq!((x, y), (0, 1));
        let (s, x, y) = wrapped_description("hello world", 11, 5);
        assert_eq!(s, "hello\nworld");
        assert_eq!((x, y), (0, 2));
        let (s, x, y) = wrapped_description("ab longword\n界", 7, 4);
        assert_eq!(s, "ab \nlong\nword\n界");
        assert_eq!((x, y), (0, 2));
        let (s, x, y) = wrapped_description("a 界界", 2, 4);
        assert_eq!(s, "a \n界界");
        assert_eq!((x, y), (0, 1));
    }
    #[test]
    fn settings_dialog_and_light_theme_render_with_contrast_at_small_sizes() {
        let mut a = app();
        a.focus_detail = true;
        a.theme = crate::settings::Theme::Light;
        let storage_error = "Could not save settings to '/example-home/.config/ontrack/settings.json': Permission denied while replacing the destination file. Check the parent directory permissions and try again.";
        a.modal = Some(crate::app::Modal::Settings {
            draft: crate::settings::Settings {
                theme: crate::settings::Theme::Light,
                ..crate::settings::Settings::default()
            },
            error: Some(storage_error.into()),
            dropdown_open: true,
            focus: 0,
        });
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal
            .draw(|f| render(f, Duration::from_secs(2), &mut a))
            .unwrap();
        let buffer = terminal.backend().buffer();
        let text = buffer
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(text.contains("SETTINGS"));
        assert!(text.contains("Light"));
        assert!(text.contains("› OnTrack Light"));
        assert!(text.contains("Close"));
        assert!(!text.contains("[ Save ]"));
        assert!(!text.contains("Cancel"));
        assert!(text.contains("settings.json"));
        assert!(text.contains("Permission") && text.contains("denied"));
        assert!(text.contains("try again"));
        assert!(text.contains("← / → choice"));
        assert!(text.contains("Dark"));
        assert!(text.contains("Enter / Space select"));
        assert!(text.contains("Esc") && text.contains("close"));
        assert!(a.modal_ready);
        assert_eq!(
            buffer[(0, 3)].bg,
            dim_color(crate::theme::definition(Theme::Light).palette.panel)
        );
        assert!(a.modal_ready);
        a.theme = Theme::Dark;
        a.modal = Some(crate::app::Modal::Settings {
            draft: crate::settings::Settings {
                theme: crate::settings::Theme::Dark,
                ..crate::settings::Settings::default()
            },
            error: None,
            dropdown_open: false,
            focus: 0,
        });
        let closed = draw((80, 24), &mut a).0;
        assert!(closed.contains("Dark"));
        assert!(closed.contains("Enter / Space open"));
        a.modal = Some(crate::app::Modal::Settings {
            draft: crate::settings::Settings {
                theme: crate::settings::Theme::Light,
                ..crate::settings::Settings::default()
            },
            error: None,
            dropdown_open: true,
            focus: 0,
        });
        assert!(draw((18, 5), &mut a).0.contains("Resize"));
        assert!(!a.modal_ready);
    }
    #[test]
    fn settings_rows_keep_wider_left_titles_and_centered_values() {
        fn locate(buffer: &ratatui::buffer::Buffer, text: &str) -> (u16, u16) {
            let chars: Vec<String> = text.chars().map(|ch| ch.to_string()).collect();
            for y in 0..buffer.area.height {
                for x in 0..buffer.area.width.saturating_sub(chars.len() as u16) {
                    if chars
                        .iter()
                        .enumerate()
                        .all(|(offset, ch)| buffer[(x + offset as u16, y)].symbol() == ch)
                    {
                        return (x, y);
                    }
                }
            }
            panic!("could not find {text:?}");
        }

        let mut state = app();
        state.modal = Some(crate::app::Modal::Settings {
            draft: crate::settings::Settings {
                auto_merge: true,
                auto_merge_children: true,
                auto_remove_checkout: true,
                ..Default::default()
            },
            error: None,
            dropdown_open: true,
            focus: 0,
        });
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal
            .draw(|f| render(f, Duration::from_secs(2), &mut state))
            .unwrap();
        let buffer = terminal.backend().buffer();
        assert_eq!(settings_columns(60), (40, 20));
        let title_x = locate(buffer, "Theme").0;
        for label in [
            "Agent limit",
            "Auto-merge top-level tasks",
            "Auto-merge child tasks into parent",
            "Close Herdr workspace after integration",
            "Remove checkout + close Herdr workspace",
        ] {
            assert_eq!(locate(buffer, label).0, title_x);
        }
        assert_eq!(title_x, 10);
        assert_eq!(locate(buffer, "OnTrack Dark ▾").0, 53);
        assert_eq!(locate(buffer, "› OnTrack Dark").0, 53);
        for label in [
            "Auto-merge top-level tasks",
            "Auto-merge child tasks into parent",
            "Remove checkout + close Herdr workspace",
        ] {
            let y = locate(buffer, label).1;
            assert_eq!(buffer[(58, y)].symbol(), "[");
            assert_eq!(buffer[(59, y)].symbol(), "x");
        }
        let workspace_y = locate(buffer, "Close Herdr workspace after integration").1;
        assert_eq!(buffer[(58, workspace_y)].symbol(), "[");
        assert_eq!(buffer[(59, workspace_y)].symbol(), " ");

        state.modal = Some(crate::app::Modal::Settings {
            draft: crate::settings::Settings::default(),
            error: None,
            dropdown_open: false,
            focus: 2,
        });
        let compact = draw((40, 18), &mut state).0;
        assert!(state.modal_ready);
        assert!(compact.contains("Theme"));
        assert!(compact.contains("Remove checkout + clo…"));
        assert!(compact.contains("[ ]"));
    }
    #[test]
    fn settings_agent_picker_shows_saved_choice_and_scrolls_to_later_kinds() {
        let mut a = app();
        a.modal = Some(crate::app::Modal::Settings {
            draft: crate::settings::Settings {
                default_agent: crate::settings::DefaultAgent::Kind("hermes"),
                ..Default::default()
            },
            error: None,
            dropdown_open: true,
            focus: 1,
        });
        let screen = draw((80, 24), &mut a).0;
        assert!(screen.contains("Default agent for new issues"));
        assert!(screen.contains("› hermes"));
        assert!(screen.contains("↑ earlier agents"));
        assert!(screen.contains("↓ later agents"));
    }
    #[test]
    fn light_create_type_dropdown_uses_catalog_selection_colors() {
        let mut a = app();
        a.theme = Theme::Light;
        a.modal = Some(crate::app::Modal::Create(crate::app::CreateForm {
            focus: 2,
            dropdown: Some(2),
            issue_type: 0,
            ..Default::default()
        }));
        let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
        terminal
            .draw(|f| render(f, Duration::ZERO, &mut a))
            .unwrap();
        let buffer = terminal.backend().buffer();
        assert!(buffer.content().iter().any(|cell| {
            cell.bg
                == crate::theme::definition(Theme::Light)
                    .palette
                    .selected_background
        }));
        assert!(!buffer.content().iter().any(|cell| cell.bg == type_color(0)));
        assert!(
            !buffer
                .content()
                .iter()
                .any(|cell| cell.fg == priority_color(2))
        );
    }
    #[test]
    fn markdown_detail_colors_and_inline_code_backgrounds_follow_theme() {
        let mut a = app();
        a.focus_detail = true;
        a.detail = Some(json!({"description":"# Overview\n\nReadable text with `inline code`."}));
        let draw_detail = |app: &mut App| {
            let mut terminal = Terminal::new(TestBackend::new(100, 24)).unwrap();
            terminal
                .draw(|frame| render(frame, Duration::from_secs(2), app))
                .unwrap();
            terminal.backend().buffer().clone()
        };
        let inline_code_position = |buffer: &ratatui::buffer::Buffer, rect: Rect| {
            (rect.y + 1..rect.bottom() - 1)
                .find_map(|y| {
                    let row = (rect.x + 1..rect.right() - 1)
                        .map(|x| buffer[(x, y)].symbol())
                        .collect::<String>();
                    row.find("inline code")
                        .map(|offset| (rect.x + 1 + row[..offset].chars().count() as u16, y))
                })
                .expect("detail text should contain the inline code row")
        };
        a.theme = Theme::Dark;
        let dark = draw_detail(&mut a);
        let dark_code_position = inline_code_position(&dark, a.detail_rect);
        a.theme = Theme::Light;
        let light = draw_detail(&mut a);
        let light_code_position = inline_code_position(&light, a.detail_rect);
        let dark_palette = crate::theme::definition(Theme::Dark).palette;
        let light_palette = crate::theme::definition(Theme::Light).palette;
        let dark_code = &dark[dark_code_position];
        let light_code = &light[light_code_position];
        assert_eq!(dark_code.fg, dark_palette.warning);
        assert_eq!(light_code.fg, light_palette.warning);
        assert_ne!(dark_code.fg, light_code.fg);
        assert_eq!(dark_code.bg, dark_palette.panel);
        assert_eq!(light_code.bg, light_palette.panel);
        assert_eq!(
            crate::theme::markdown_foreground(Theme::SolarizedLight, Color::Indexed(1)),
            crate::theme::definition(Theme::SolarizedLight)
                .palette
                .danger
        );
    }
    #[test]
    fn every_theme_renders_splash_and_browser_at_normal_and_compact_sizes() {
        for theme in Theme::ALL {
            let palette = crate::theme::definition(theme).palette;
            let mut app = app();
            app.theme = theme;
            app.beads[0].status = "open".into();
            app.focused_column = 0;
            for size in [(100, 30), (60, 18)] {
                let rendered = draw(size, &mut app).0;
                assert!(
                    rendered.contains("Title is visible"),
                    "{theme:?} at {size:?}"
                );
                assert!(rendered.contains("ONTRACK"), "{theme:?} at {size:?}");
                app.modal = Some(crate::app::Modal::Settings {
                    draft: crate::settings::Settings {
                        theme,
                        ..crate::settings::Settings::default()
                    },
                    error: None,
                    dropdown_open: true,
                    focus: 0,
                });
                let settings = draw(size, &mut app).0;
                assert!(settings.contains("SETTINGS"), "{theme:?} at {size:?}");
                assert!(
                    app.modal_ready,
                    "settings modal should fit for {theme:?} at {size:?}"
                );
                app.modal = None;
            }
            let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
            terminal
                .draw(|frame| render(frame, Duration::from_secs(2), &mut app))
                .unwrap();
            let buffer = terminal.backend().buffer();
            assert!(
                buffer
                    .content()
                    .iter()
                    .any(|cell| cell.bg == palette.background)
            );
            assert!(
                buffer
                    .content()
                    .iter()
                    .any(|cell| cell.fg == palette.success)
            );
            assert_eq!(
                crate::theme::markdown_foreground(theme, Color::Rgb(220, 228, 229)),
                palette.foreground
            );
        }
    }
    #[test]
    fn theme_dropdown_keeps_late_selection_visible_and_marks_more_options() {
        let mut app = app();
        app.modal = Some(crate::app::Modal::Settings {
            draft: crate::settings::Settings {
                theme: crate::settings::Theme::System,
                ..crate::settings::Settings::default()
            },
            error: None,
            dropdown_open: true,
            focus: 0,
        });
        let late = draw((80, 24), &mut app).0;
        assert!(late.contains("System"));
        assert!(!late.contains("↓ later themes"));
        assert!(late.contains("↑ earlier themes"));
        assert!(!late.contains("OnTrack Dark"));
        assert!(app.modal_ready);

        app.modal = Some(crate::app::Modal::Settings {
            draft: crate::settings::Settings {
                theme: crate::settings::Theme::TokyoNight,
                ..crate::settings::Settings::default()
            },
            error: None,
            dropdown_open: true,
            focus: 0,
        });
        let early = draw((80, 24), &mut app).0;
        assert!(early.contains("Tokyo Night"));
        assert!(early.contains("↓ later themes"));
        assert!(early.contains("Close"));
        assert!(!early.contains("Save"));
        assert!(!early.contains("Cancel"));
        assert!(app.modal_ready);
    }
    #[test]
    fn agent_limit_dropdown_shows_unlimited_then_increasing_counts() {
        let mut app = app();
        app.modal = Some(crate::app::Modal::Settings {
            draft: crate::settings::Settings::default(),
            error: None,
            dropdown_open: true,
            focus: 2,
        });
        let screen = draw((80, 30), &mut app).0;
        let labels = ["› 5", "  Unlimited", "  1", "  2", "  3", "  4"];
        for label in labels {
            assert!(screen.contains(label), "missing {label:?}");
        }
        assert!(screen.find("Unlimited").unwrap() < screen.find("› 5").unwrap());
        assert!(screen.contains("↓ later limits"));

        app.modal = Some(crate::app::Modal::Settings {
            draft: crate::settings::Settings {
                agent_limit: crate::settings::AgentLimit::Limited(12),
                ..Default::default()
            },
            error: None,
            dropdown_open: true,
            focus: 2,
        });
        let later = draw((80, 24), &mut app).0;
        assert!(later.contains("› 12"));
        assert!(later.contains("↑ earlier limits"));
        assert!(later.contains("↓ later limits"));
    }
    #[test]
    fn wide_title_horizontal_scroll_starts_at_character_boundary() {
        let title = "界".repeat(40);
        let caret = Span::raw(title.as_str()).width();
        let desired = caret.saturating_sub(9);
        let (window, start) = display_window(&title, desired, 10);
        assert_eq!(start % 2, 0);
        assert!(Span::raw(window.as_str()).width() <= 10);
        assert!(caret - start < 10);
        let (_, start) = display_window("a界b", 2, 3);
        assert_eq!(start, 3);
    }
    #[test]
    fn handoff_toast_is_top_right_and_does_not_open_a_dialog() {
        for size in [(80, 24), (30, 9)] {
            let mut app = app();
            app.toast = Some(crate::app::Toast {
                message: "Finding Herdr agent…".into(),
                error: false,
                expires_at: None,
            });
            let mut terminal = Terminal::new(TestBackend::new(size.0, size.1)).unwrap();
            terminal
                .draw(|frame| render(frame, Duration::from_secs(2), &mut app))
                .unwrap();
            let buffer = terminal.backend().buffer();
            let top = (0..size.0)
                .map(|x| buffer[(x, 0)].symbol())
                .collect::<String>();
            assert!(
                top.contains("HERDR"),
                "toast must touch the top edge at {size:?}"
            );
            assert!(
                top.find("HERDR").unwrap() > 0,
                "toast must be on the right at {size:?}"
            );
            assert!(app.modal.is_none());
            let screen = buffer
                .content()
                .iter()
                .map(|cell| cell.symbol())
                .collect::<String>();
            assert!(screen.contains("Finding"));
        }
    }
    #[test]
    fn expired_handoff_toast_disappears_and_compact_error_stays_readable() {
        let mut app = app();
        app.toast = Some(crate::app::Toast {
            message: "Handoff failed: inspect recorded worktree before retrying".into(),
            error: true,
            expires_at: Some(std::time::Instant::now() + Duration::from_secs(5)),
        });
        let visible = draw((40, 12), &mut app).0;
        assert!(visible.contains("Handoff"));
        assert!(visible.contains("failed"));
        app.toast.as_mut().unwrap().expires_at =
            Some(std::time::Instant::now() - Duration::from_secs(1));
        let expired = draw((40, 12), &mut app).0;
        assert!(!expired.contains("Handoff failed"));
    }

    #[test]
    fn wide_board_renders_all_columns_counts_cards_and_exact_hit_rectangles() {
        let mut a = app();
        a.beads = crate::app::BOARD_STATUSES
            .iter()
            .enumerate()
            .map(|(i, status)| Bead {
                dependency_ready: true,
                blocked_by: Vec::new(),
                id: format!("b{i}"),
                title: format!("Card {i}"),
                status: match *status {
                    "ready" => "open".into(),
                    "ready_to_merge" => "in_progress".into(),
                    value => value.into(),
                },
                priority: i as u64 % 5,
                data: match *status {
                    "ready" => json!({"issue_type":"task","labels":[crate::beads::READY_LABEL]}),
                    "ready_to_merge" => {
                        json!({"issue_type":"task","status":"in_progress","labels":[crate::beads::READY_TO_MERGE_LABEL]})
                    }
                    _ => json!({"issue_type":"task"}),
                },
            })
            .collect();
        a.selected = Some(4);
        a.focused_column = 4;
        let (screen, _) = draw((160, 30), &mut a);
        assert_eq!(a.board_columns.len(), 6);
        for (label, title) in [
            ("OPEN · 1", "Card 0"),
            ("READY · 1", "Card 1"),
            ("BLOCKED · 1", "Card 2"),
            ("IN PROGRESS · 1", "Card 3"),
            ("READY TO MERGE · 1", "Card 4"),
            ("CLOSED · 1", "Card 5"),
        ] {
            let (heading, count) = label.split_once(" · ").unwrap();
            assert!(
                screen.contains(heading) && screen.contains(count),
                "missing {label}: {screen}"
            );
            assert!(screen.contains(title), "missing {title}");
        }
        assert_eq!(a.board_cards.len(), 6);
        for (_, rect) in &a.board_cards {
            assert!(
                a.board_columns
                    .iter()
                    .any(|(_, column)| { rect.x >= column.x && rect.right() <= column.right() })
            );
        }
    }

    #[test]
    fn split_pane_board_keeps_full_headings_and_cards_readable() {
        let mut a = overflowing_board();
        let (screen, _) = draw((112, 55), &mut a);
        for (column, status) in crate::app::BOARD_STATUSES.iter().enumerate() {
            assert!(screen.contains(status_label(status)));
            assert!(screen.contains(&format!("Card {column}-00")));
            let rect = a.board_columns[column].1;
            assert!(rect.width >= 37);
            assert_eq!(rect.y, if column < 3 { 2 } else { 28 });
        }
    }

    #[test]
    fn board_grid_reflows_at_width_boundaries_and_tiles_offset_area() {
        for (width, columns) in [
            (24, 2),
            (53, 2),
            (54, 3),
            (55, 3),
            (107, 3),
            (108, 3),
            (109, 3),
            (112, 3),
            (120, 3),
            (143, 3),
            (144, 6),
            (145, 6),
        ] {
            let mut a = overflowing_board();
            let area = Rect::new(3, 2, width, 49);
            let mut terminal = Terminal::new(TestBackend::new(width + 6, 53)).unwrap();
            terminal.draw(|f| render_kanban(f, area, &mut a)).unwrap();
            assert_eq!(a.board_columns.len(), 6);
            let mut cells = std::collections::HashSet::new();
            for (column, &(status, rect)) in a.board_columns.iter().enumerate() {
                assert_eq!(column, status);
                assert_eq!(rect.x, a.board_columns[column % columns].1.x);
                assert_eq!(rect.y, a.board_columns[column / columns * columns].1.y);
                for y in rect.y..rect.bottom() {
                    for x in rect.x..rect.right() {
                        assert!(area.contains((x, y).into()));
                        assert!(cells.insert((x, y)), "overlapping columns");
                    }
                }
                let cards = a
                    .board_cards
                    .iter()
                    .filter(|(i, _)| i / 12 == column)
                    .collect::<Vec<_>>();
                assert!(!cards.is_empty());
                for (_, card) in cards {
                    assert!(card.x > rect.x && card.right() < rect.right());
                    assert!(card.y > rect.y && card.bottom() < rect.bottom());
                }
            }
            assert_eq!(cells.len(), usize::from(width) * 49);
        }
        for width in [0, 1, 2, 24, 53, 54, 107, 108, 143, 144] {
            for height in 0..8 {
                let mut a = overflowing_board();
                a.board_cards.push((999, Rect::new(1, 1, 1, 1)));
                let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
                terminal
                    .draw(|f| render_kanban(f, f.area(), &mut a))
                    .unwrap();
                assert!(a.board_cards.is_empty(), "{width}x{height}");
            }
        }
    }

    #[test]
    fn grid_resize_preserves_selection_and_lower_row_mouse_navigation() {
        use crossterm::event::{KeyCode, MouseButton, MouseEventKind};

        let mut a = overflowing_board();
        a.focused_column = 4;
        a.selected = Some(56);
        a.column_rows[4] = 8;
        a.column_offsets[4] = 7;
        for size in [
            (144, 50),
            (143, 50),
            (112, 50),
            (80, 50),
            (40, 50),
            (40, 9),
            (112, 50),
            (144, 50),
        ] {
            draw(size, &mut a);
            assert_eq!(a.selected, Some(56));
            assert_eq!(a.focused_column, 4);
            if size.1 == 50 {
                assert!(a.board_cards.iter().any(|(i, _)| *i == 56));
            } else {
                assert!(a.board_cards.is_empty());
            }
        }
        for size in [(112, 50), (80, 50), (40, 50)] {
            draw(size, &mut a);
            let (index, card) = *a.board_cards.iter().find(|(i, _)| i / 12 == 5).unwrap();
            a.mouse(
                MouseEventKind::Down(MouseButton::Left),
                card.x + 1,
                card.y + 1,
            );
            assert_eq!(a.focused_column, 5);
            assert_eq!(a.selected, Some(index));
            a.mouse(MouseEventKind::ScrollDown, card.x + 1, card.y + 1);
            assert_eq!(a.selected, Some(index + 1));
            board_key(&mut a, KeyCode::Left);
            assert_eq!(a.focused_column, 4);
            board_key(&mut a, KeyCode::Right);
            assert_eq!(a.selected, Some(index + 1));
        }
    }

    #[test]
    fn board_cards_and_hit_rectangles_stay_inside_bottom_column_border() {
        for height in 8..=24 {
            let mut a = overflowing_board();
            let mut terminal = Terminal::new(TestBackend::new(120, height)).unwrap();
            terminal
                .draw(|frame| render_kanban(frame, frame.area(), &mut a))
                .unwrap();
            for (_, card) in &a.board_cards {
                let column = a
                    .board_columns
                    .iter()
                    .find(|(_, column)| column.contains((card.x, card.y).into()))
                    .unwrap()
                    .1;
                assert!(
                    card.x > column.x
                        && card.right() < column.right()
                        && card.y > column.y
                        && card.bottom() < column.bottom(),
                    "height {height}: {card:?} in {column:?}"
                );
            }
            let buffer = terminal.backend().buffer();
            for (_, column) in &a.board_columns {
                assert_eq!(buffer[(column.x, column.bottom() - 1)].symbol(), "╰");
                assert_eq!(
                    buffer[(column.right() - 1, column.bottom() - 1)].symbol(),
                    "╯"
                );
                for x in column.x + 1..column.right() - 1 {
                    assert_eq!(buffer[(x, column.bottom() - 1)].symbol(), "─");
                }
            }
        }

        let mut a = overflowing_board();
        let mut terminal = Terminal::new(TestBackend::new(72, 7)).unwrap();
        terminal
            .draw(|frame| render_kanban(frame, frame.area(), &mut a))
            .unwrap();
        assert!(a.board_cards.is_empty());
        for row in a.board_columns.chunks(3) {
            for pair in row.windows(2) {
                assert_eq!(pair[0].1.right(), pair[1].1.x);
            }
        }
        let buffer = terminal.backend().buffer();
        for (_, column) in &a.board_columns {
            assert_eq!(buffer[(column.x, column.y)].symbol(), "╭");
        }
    }

    #[test]
    fn resizing_keeps_selected_card_visible_across_board_boundaries() {
        use crossterm::event::KeyCode;

        let mut a = overflowing_board();
        let mut terminal = Terminal::new(TestBackend::new(144, 24)).unwrap();
        terminal
            .draw(|frame| render_kanban(frame, frame.area(), &mut a))
            .unwrap();
        for _ in 0..3 {
            board_key(&mut a, KeyCode::Down);
            terminal
                .draw(|frame| render_kanban(frame, frame.area(), &mut a))
                .unwrap();
        }
        assert_eq!(a.selected, Some(3));
        let mut small = Terminal::new(TestBackend::new(144, 12)).unwrap();
        small
            .draw(|frame| render_kanban(frame, frame.area(), &mut a))
            .unwrap();
        assert!(
            a.board_cards
                .iter()
                .any(|(index, _)| Some(*index) == a.selected)
        );
        assert_eq!(a.column_offsets[0], 3);
        board_event(&mut a, board_navigation_events()[0].0.clone());
        small
            .draw(|frame| render_kanban(frame, frame.area(), &mut a))
            .unwrap();
        assert_eq!(a.selected, Some(4));
        assert!(
            a.board_cards
                .iter()
                .any(|(index, _)| Some(*index) == a.selected)
        );
        let mut large = Terminal::new(TestBackend::new(144, 24)).unwrap();
        large
            .draw(|frame| render_kanban(frame, frame.area(), &mut a))
            .unwrap();
        assert!(
            a.board_cards
                .iter()
                .any(|(index, _)| Some(*index) == a.selected)
        );
        board_event(&mut a, board_navigation_events()[1].0.clone());
        small
            .draw(|frame| render_kanban(frame, frame.area(), &mut a))
            .unwrap();
        assert_eq!(a.selected, Some(5));
        assert!(
            a.board_cards
                .iter()
                .any(|(index, _)| Some(*index) == a.selected)
        );
    }

    #[test]
    fn kanban_outer_bottom_borders_keep_status_colors_and_cards_stay_inside() {
        for theme in Theme::ALL {
            for size in [
                (160, 11),
                (160, 7),
                (160, 10),
                (160, 15),
                (160, 20),
                (160, 30),
                (80, 15),
            ] {
                let mut a = app();
                a.theme = theme;
                a.focused_column = 5;
                a.beads = crate::app::BOARD_STATUSES.iter().enumerate().flat_map(|(i, status)| {
                    (0..3).map(move |n| Bead {
                            dependency_ready: true,
                            blocked_by: Vec::new(),
                            id: format!("b{i}-{n}"),
                            title: format!("Card {i}-{n}"),
                            status: match *status {
                                "ready" => "open".into(),
                                "ready_to_merge" => "in_progress".into(),
                                value => value.into(),
                            },
                            priority: 0,
                            data: match *status {
                                "ready" => json!({"issue_type":"task","labels":[crate::beads::READY_LABEL]}),
                                "ready_to_merge" => json!({"issue_type":"task","status":"in_progress","labels":[crate::beads::READY_TO_MERGE_LABEL]}),
                                _ => json!({"issue_type":"task"}),
                            },
                        })
                }).collect();
                let mut terminal = Terminal::new(TestBackend::new(size.0, size.1)).unwrap();
                terminal
                    .draw(|f| render(f, Duration::from_secs(2), &mut a))
                    .unwrap();
                let buffer = terminal.backend().buffer();
                assert_eq!(a.board_columns.len(), 6);
                for row in a.board_columns.chunks(if size.0 == 80 { 3 } else { 6 }) {
                    assert!(row.windows(2).all(|pair| pair[0].1.right() == pair[1].1.x));
                }
                let mut total_cards = 0;
                for &(column, rect) in &a.board_columns {
                    let color =
                        crate::theme::status_color(theme, crate::app::BOARD_STATUSES[column]);
                    let bottom = rect.bottom() - 1;
                    assert_eq!(buffer[(rect.x, bottom)].symbol(), "╰");
                    assert_eq!(buffer[(rect.right() - 1, bottom)].symbol(), "╯");
                    for x in rect.x + 1..rect.right() - 1 {
                        assert_eq!(buffer[(x, bottom)].symbol(), "─");
                    }
                    for x in rect.x..rect.right() {
                        assert_eq!(
                            buffer[(x, bottom)].fg,
                            color,
                            "{theme:?} {size:?} column {column}"
                        );
                    }
                    let cards = a
                        .board_cards
                        .iter()
                        .filter(|(_, card)| {
                            rect.contains(card.as_position()) && card.bottom() <= rect.bottom()
                        })
                        .count();
                    let expected = (rect.height.saturating_sub(3) as usize / 5).min(3);
                    assert_eq!(cards, expected, "{theme:?} {size:?} column {column}");
                    total_cards += cards;
                }
                assert_eq!(total_cards, a.board_cards.len());
                for (index, card) in &a.board_cards {
                    let column = a
                        .board_columns
                        .iter()
                        .find(|(_, rect)| {
                            rect.contains(card.as_position()) && card.bottom() <= rect.bottom()
                        })
                        .unwrap()
                        .1;
                    assert!(
                        card.bottom() < column.bottom(),
                        "card overlaps bottom border at {size:?}"
                    );
                    let palette = crate::theme::definition(theme).palette;
                    for y in card.y..card.bottom() {
                        for x in card.x..card.right() {
                            let selected_interior = a.selected == Some(*index)
                                && x > card.x
                                && x + 1 < card.right()
                                && y > card.y
                                && y + 1 < card.bottom();
                            let expected = if selected_interior {
                                palette.selected_background
                            } else {
                                buffer[(column.x, column.y + 1)].bg
                            };
                            assert_eq!(
                                buffer[(x, y)].bg,
                                expected,
                                "{theme:?} {size:?} card {index} at ({x}, {y})"
                            );
                        }
                    }
                }
                a.beads.clear();
                terminal
                    .draw(|f| render(f, Duration::from_secs(2), &mut a))
                    .unwrap();
                let buffer = terminal.backend().buffer();
                assert!(a.board_cards.is_empty());
                for &(column, rect) in &a.board_columns {
                    let bottom = rect.bottom() - 1;
                    let color =
                        crate::theme::status_color(theme, crate::app::BOARD_STATUSES[column]);
                    assert_eq!(buffer[(rect.x, bottom)].symbol(), "╰");
                    assert_eq!(buffer[(rect.right() - 1, bottom)].symbol(), "╯");
                    for x in rect.x..rect.right() {
                        assert_eq!(buffer[(x, bottom)].fg, color);
                    }
                }
            }
        }
    }

    #[test]
    fn board_navigation_arrow_keys_cross_viewport_edges_one_card_at_a_time() {
        assert_board_navigation_edges(board_navigation_events()[0].clone());
    }

    #[test]
    fn board_navigation_wheel_crosses_viewport_edges_one_card_at_a_time() {
        assert_board_navigation_edges(board_navigation_events()[1].clone());
    }

    fn assert_board_navigation_edges(
        (down, up): (crossterm::event::Event, crossterm::event::Event),
    ) {
        let mut a = overflowing_board();
        draw((144, 24), &mut a);
        assert_board_viewport(&a, 0, 0);
        for row in 1..=2 {
            board_event(&mut a, down.clone());
            draw((144, 24), &mut a);
            assert_eq!(a.selected, Some(row), "after {down:?}");
            assert_board_viewport(&a, 0, 0);
        }
        let bottom = a.board_cards.iter().find(|(i, _)| *i == 2).unwrap().1;
        for row in 3..=4 {
            board_event(&mut a, down.clone());
            let screen = draw((144, 24), &mut a).0;
            assert_eq!(a.selected, Some(row), "after {down:?}");
            assert_board_viewport(&a, 0, row - 2);
            assert_eq!(
                a.board_cards.iter().find(|(i, _)| *i == row).unwrap().1,
                bottom
            );
            assert!(screen.contains(&format!("Card 0-{row:02}")));
        }
        for row in (0..4).rev() {
            board_event(&mut a, up.clone());
            draw((144, 24), &mut a);
            assert_eq!(a.selected, Some(row), "after {up:?}");
            assert_board_viewport(&a, 0, row.min(2));
            if row < 2 {
                assert_eq!(
                    a.board_cards.iter().find(|(i, _)| *i == row).unwrap().1.y,
                    bottom.y - 10
                );
            }
        }
        board_event(&mut a, up);
        draw((144, 24), &mut a);
        assert_eq!(a.selected, Some(0));
        assert_board_viewport(&a, 0, 0);
    }

    #[test]
    fn board_navigation_does_not_wrap_at_first_or_last_card() {
        for (down, up) in board_navigation_events() {
            let mut a = overflowing_board();
            draw((144, 24), &mut a);
            for step in 1usize..=14 {
                board_event(&mut a, down.clone());
                draw((144, 24), &mut a);
                let row = step.min(11);
                assert_eq!(a.selected, Some(row));
                assert_board_viewport(&a, 0, row.saturating_sub(2));
                assert_eq!(&a.column_offsets[1..], &[0; 5]);
            }
            for step in 1usize..=14 {
                board_event(&mut a, up.clone());
                draw((144, 24), &mut a);
                let row = 11usize.saturating_sub(step);
                assert_eq!(a.selected, Some(row));
                assert_board_viewport(&a, 0, row.min(9));
                assert_eq!(&a.column_offsets[1..], &[0; 5]);
            }
        }
    }

    #[test]
    fn board_navigation_batches_events_without_using_stale_card_positions() {
        for (down, up) in board_navigation_events() {
            let mut a = overflowing_board();
            draw((144, 24), &mut a);
            for _ in 0..2 {
                board_event(&mut a, down.clone());
            }
            draw((144, 24), &mut a);
            assert_eq!(a.selected, Some(2));
            assert_board_viewport(&a, 0, 0);
            for _ in 0..3 {
                board_event(&mut a, down.clone());
            }
            draw((144, 24), &mut a);
            assert_eq!(a.selected, Some(5));
            assert_board_viewport(&a, 0, 3);
            for _ in 0..5 {
                board_event(&mut a, up.clone());
            }
            draw((144, 24), &mut a);
            assert_eq!(a.selected, Some(0));
            assert_board_viewport(&a, 0, 0);
        }
    }

    #[test]
    fn board_navigation_retains_independent_offsets_across_grid_rows() {
        use crossterm::event::KeyCode;

        for size in [(144, 24), (120, 45), (60, 45), (36, 66)] {
            let mut a = overflowing_board();
            draw(size, &mut a);
            for _ in 0..6 {
                board_key(&mut a, KeyCode::Down);
                draw(size, &mut a);
            }
            assert_eq!(a.selected, Some(6));
            assert_board_viewport(&a, 0, 4);
            board_key(&mut a, KeyCode::Right);
            draw(size, &mut a);
            for _ in 0..4 {
                board_key(&mut a, KeyCode::Down);
                draw(size, &mut a);
            }
            assert_eq!(a.selected, Some(16));
            assert_board_viewport(&a, 1, 2);
            assert_eq!(a.column_offsets[0], 4);
            for _ in 0..4 {
                board_key(&mut a, KeyCode::Right);
                draw(size, &mut a);
            }
            assert_eq!(a.focused_column, 5);
            for _ in 0..5 {
                board_key(&mut a, KeyCode::Down);
                draw(size, &mut a);
            }
            assert_board_viewport(&a, 5, 3);
            assert_eq!(a.column_offsets, [4, 2, 0, 0, 0, 3]);
            for _ in 0..4 {
                board_key(&mut a, KeyCode::Left);
                draw(size, &mut a);
            }
            assert_eq!(a.selected, Some(16));
            assert_board_viewport(&a, 1, 2);
            board_key(&mut a, KeyCode::Left);
            draw(size, &mut a);
            assert_eq!(a.selected, Some(6));
            assert_board_viewport(&a, 0, 4);
            assert_eq!(a.column_offsets, [4, 2, 0, 0, 0, 3]);
        }
    }

    #[test]
    fn board_navigation_is_safe_when_the_viewport_cannot_fit_a_card() {
        use crossterm::event::KeyCode;

        for size in [(120, 9), (24, 7), (12, 4), (1, 1)] {
            let mut a = overflowing_board();
            draw(size, &mut a);
            assert!(a.board_cards.is_empty());
            for (code, down) in [(KeyCode::Down, true), (KeyCode::Up, false)] {
                for step in 1usize..=14 {
                    board_key(&mut a, code);
                    draw(size, &mut a);
                    let row = if down {
                        step.min(11)
                    } else {
                        11usize.saturating_sub(step)
                    };
                    assert_eq!(a.selected, Some(row), "tiny viewport {size:?}");
                    assert!(a.board_cards.is_empty());
                    assert!(a.column_offsets.iter().all(|offset| *offset < 12));
                }
            }
            draw((144, 24), &mut a);
            assert_eq!(a.selected, Some(0));
            assert_board_viewport(&a, 0, 0);
        }
    }

    fn overflowing_board() -> App {
        let mut a = app();
        a.beads = crate::app::BOARD_STATUSES
            .iter()
            .enumerate()
            .flat_map(|(column, status)| {
                (0..12).map(move |row| Bead {
                    dependency_ready: true,
                    blocked_by: Vec::new(),
                    id: format!("c{column}-{row:02}"),
                    title: format!("Card {column}-{row:02}"),
                    status: match *status {
                        "ready" => "open",
                        "ready_to_merge" => "in_progress",
                        value => value,
                    }
                    .into(),
                    priority: 2,
                    data: match *status {
                        "ready" => json!({"issue_type":"task","labels":[crate::beads::READY_LABEL]}),
                        "ready_to_merge" => json!({"issue_type":"task","status":"in_progress","labels":[crate::beads::READY_TO_MERGE_LABEL]}),
                        _ => json!({"issue_type":"task"}),
                    },
                })
            })
            .collect();
        a.focused_column = 0;
        a.selected = Some(0);
        a
    }

    fn board_navigation_events() -> [(crossterm::event::Event, crossterm::event::Event); 2] {
        use crossterm::event::{
            Event, KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind,
        };
        let wheel = |kind| {
            Event::Mouse(MouseEvent {
                kind,
                column: 2,
                row: 5,
                modifiers: KeyModifiers::NONE,
            })
        };
        [
            (
                Event::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)),
                Event::Key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE)),
            ),
            (
                wheel(MouseEventKind::ScrollDown),
                wheel(MouseEventKind::ScrollUp),
            ),
        ]
    }

    fn board_event(a: &mut App, event: crossterm::event::Event) {
        match event {
            crossterm::event::Event::Key(key) => assert!(!a.key(key)),
            crossterm::event::Event::Mouse(mouse) => a.mouse(mouse.kind, mouse.column, mouse.row),
            _ => unreachable!(),
        }
    }

    fn board_key(a: &mut App, code: crossterm::event::KeyCode) {
        use crossterm::event::{Event, KeyEvent, KeyModifiers};
        board_event(a, Event::Key(KeyEvent::new(code, KeyModifiers::NONE)));
    }

    fn assert_board_viewport(a: &App, column: usize, offset: usize) {
        assert_eq!(a.column_offsets[column], offset, "column {column} offset");
        let cards = a
            .board_cards
            .iter()
            .filter(|(i, _)| i / 12 == column)
            .collect::<Vec<_>>();
        assert_eq!(
            cards.iter().map(|(i, _)| *i).collect::<Vec<_>>(),
            (column * 12 + offset..column * 12 + offset + 3).collect::<Vec<_>>()
        );
        for pair in cards.windows(2) {
            assert_eq!(
                pair[0].1.bottom(),
                pair[1].1.y,
                "visible cards must be contiguous"
            );
        }
        if a.focused_column == column {
            assert!(
                cards.iter().any(|(index, _)| Some(*index) == a.selected),
                "selected card must remain visible"
            );
        }
    }

    #[test]
    fn narrow_board_keeps_all_columns_visible_and_column_scroll_independent() {
        let mut a = app();
        a.beads = (0..12)
            .map(|i| Bead {
                dependency_ready: true,
                blocked_by: Vec::new(),
                id: format!("open-{i}"),
                title: format!("Open card {i}"),
                status: "open".into(),
                priority: 2,
                data: json!({"issue_type":"task"}),
            })
            .collect();
        a.focused_column = 5;
        a.selected = None;
        a.column_offsets[0] = 8;
        draw((60, 20), &mut a);
        assert_eq!(a.board_columns.len(), 6);
        assert!(a.board_columns.iter().any(|(column, _)| *column == 5));
        assert_eq!(a.column_offsets[0], 8);

        a.focused_column = 0;
        draw((60, 20), &mut a);
        assert!(a.board_columns.iter().any(|(column, _)| *column == 0));
        assert!(a.board_cards.iter().any(|(index, _)| *index == 8));
    }

    #[test]
    fn detail_drawer_overlays_board_without_changing_selection_or_hit_rects() {
        let mut a = app();
        a.beads[0].status = "open".into();
        a.focused_column = 0;
        a.detail = Some(json!({"id":"demo-1","description":"Drawer details"}));
        a.focus_detail = true;
        let selected = a.selected;
        let (screen, _) = draw((120, 28), &mut a);
        assert!(screen.contains("› OPEN"));
        assert!(screen.contains("BEAD DETAILS"));
        assert!(screen.contains("Drawer details"));
        assert_eq!(a.selected, selected);
        assert!(a.board_columns.iter().any(|(column, _)| *column == 0));
        assert!(a.detail_rect.width > 0);
        assert!(
            a.board_cards
                .iter()
                .any(|(index, rect)| { *index == selected.unwrap() && rect.width > 0 })
        );
    }

    #[test]
    fn worktree_drawer_keeps_handoff_details_controls_and_modal_above_it() {
        let mut a = app();
        a.worktrees_view = true;
        a.beads.clear();
        a.worktrees.push(crate::worktrees::ManagedWorktree {
            id: "wt-1".into(),
            bead_id: "demo-1".into(),
            parent_bead_id: None,
            handoff_accepted: true,
            bead_title: Some("Captured title".into()),
            bead_description: Some("Captured handoff notes".into()),
            branch: "ontrack/demo-1".into(),
            target_branch: "main".into(),
            workspace_id: "workspace-1".into(),
            status: crate::worktrees::WorktreeStatus::NeedsAttention,
            detail: Some("Review needed".into()),
        });
        let screen = draw((120, 30), &mut a).0;
        for text in [
            "WORKTREES",
            "HANDOFFS",
            "HANDOFF DETAILS",
            "Captured title",
            "Review needed",
            "m merge",
            "R restart",
        ] {
            assert!(screen.contains(text), "missing {text}");
        }
        assert!(a.list_rect.width > 0 && a.detail_rect.width > 0);
        a.modal = Some(crate::app::Modal::ConfirmWorktree {
            id: "wt-1".into(),
            action: 4,
        });
        let confirmed = draw((120, 30), &mut a).0;
        assert!(confirmed.contains("CONTINUE TASK?"));
        assert!(confirmed.contains("Existing changes are preserved"));
    }

    #[test]
    fn move_chooser_overlays_board_and_shows_all_fixed_destinations() {
        let mut a = app();
        a.modal = Some(crate::app::Modal::Move {
            id: "demo-1".into(),
            title: "Move this bead".into(),
            selected: 4,
        });
        let screen = draw((100, 30), &mut a).0;
        assert!(screen.contains("MOVE BEAD"));
        assert!(screen.contains("Move this bead"));
        assert!(screen.contains("READY TO MERGE"));
        assert!(screen.contains("↑/↓ choose · Enter move · Esc cancel"));
        assert!(screen.contains("CLOSED"));
    }

    #[test]
    fn both_drawers_follow_board_breakpoints_when_resizing() {
        for worktrees in [false, true] {
            let mut a = app();
            a.focus_detail = !worktrees;
            a.worktrees_view = worktrees;
            a.settings.animate_drawers = false;
            let selected = a.selected;
            for (width, columns, expected) in [
                (180, 6, 72),
                (144, 6, 57),
                (143, 3, 100),
                (100, 3, 70),
                (54, 3, 37),
                (53, 2, 53),
                (144, 6, 57),
            ] {
                assert_eq!(board_columns(width), columns);
                assert_eq!(drawer_width(width, &a, Instant::now()), expected);
                let text = draw((width, 30), &mut a).0;
                let rect = a.detail_rect;
                let inset = u16::from(worktrees);
                assert_eq!(rect.width, expected - 2 * inset);
                assert_eq!(rect.x, width - expected + inset);
                assert_eq!(rect.right(), width - inset);
                assert!(text.contains(if worktrees {
                    "WORKTREES"
                } else {
                    "BEAD DETAILS"
                }));
                assert_eq!(a.selected, selected);
                assert_eq!(a.board_columns.len(), 6);
            }
        }
        let a = app();
        for width in [0, 1, 3, 20, u16::MAX] {
            assert!(drawer_width(width, &a, Instant::now()) <= width);
        }
    }

    #[test]
    fn drawer_width_eases_in_both_directions_and_can_be_disabled() {
        let start = Instant::now();
        let mut a = app();
        for (from, to) in [(0.0, 1.0), (1.0, 0.0)] {
            a.drawer_animation = Some(crate::app::DrawerAnimation {
                worktrees: false,
                started: start,
                from,
                to,
            });
            assert_eq!(drawer_width(100, &a, start), (70.0 * from) as u16);
            let middle = drawer_width(100, &a, start + Duration::from_millis(110));
            assert!(middle > 0 && middle < 70);
            assert_eq!(
                drawer_width(100, &a, start + Duration::from_millis(220)),
                (70.0 * to) as u16
            );
        }
        a.settings.animate_drawers = false;
        assert_eq!(drawer_width(100, &a, start), 70);
    }

    #[test]
    fn both_drawers_remain_visible_while_closing_then_disappear() {
        for worktrees in [false, true] {
            let mut a = app();
            let start = Instant::now();
            a.drawer_animation = Some(crate::app::DrawerAnimation {
                worktrees,
                started: start,
                from: 1.0,
                to: 0.0,
            });
            let title = if worktrees {
                "WORKTREES"
            } else {
                "BEAD DETAILS"
            };
            assert!(draw_drawer((100, 30), &mut a, start).contains(title));
            assert_eq!(a.detail_rect.width, if worktrees { 68 } else { 70 });
            let middle = start + Duration::from_millis(40);
            assert!(draw_drawer((100, 30), &mut a, middle).contains(title));
            assert!(a.detail_rect.width > 0 && a.detail_rect.width < 70);
            for size in [(60, 18), (20, 8), (100, 30)] {
                draw_drawer(size, &mut a, middle);
                assert!(a.detail_rect.width > 0 && a.detail_rect.width <= size.0);
                assert!(a.detail_rect.right() <= size.0);
            }
            let complete = start + Duration::from_millis(220);
            assert_eq!(drawer_width(100, &a, complete), 0);
            assert!(!draw_drawer((100, 30), &mut a, complete).contains(title));
            a.settings.animate_drawers = false;
            assert!(!draw_drawer((100, 30), &mut a, start).contains(title));
        }
    }

    #[test]
    fn both_drawers_render_through_animation_resize_and_static_geometry() {
        for worktrees in [false, true] {
            let mut a = app();
            a.focus_detail = !worktrees;
            a.worktrees_view = worktrees;
            a.detail_offset = 7;
            let start = Instant::now();
            a.drawer_animation = Some(crate::app::DrawerAnimation {
                worktrees,
                started: start,
                from: 0.0,
                to: 1.0,
            });

            let initial_text = draw_drawer((100, 30), &mut a, start);
            let initial = a.detail_rect;
            assert!(initial.width < 4);
            assert_eq!(a.detail_offset, 7, "unusable frames must not clamp scroll");
            assert!(initial.x + initial.width <= 100);
            assert!(a.list_rect.x + a.list_rect.width <= 100);
            assert!(!initial_text.contains(if worktrees {
                "WORKTREES ·"
            } else {
                "BEAD DETAILS"
            }));

            let halfway = start + Duration::from_millis(110);
            a.drawer_animation = Some(crate::app::DrawerAnimation {
                worktrees,
                started: start,
                from: 0.0,
                to: 1.0,
            });
            let middle_text = draw_drawer((60, 18), &mut a, halfway);
            assert!(a.detail_rect.width > 0 && a.detail_rect.width < 42);
            assert!(a.detail_rect.x + a.detail_rect.width <= 60);
            assert!(a.list_rect.x + a.list_rect.width <= 60);
            assert!(middle_text.contains(if worktrees {
                "WORKTREES"
            } else {
                "BEAD DETAILS"
            }));

            let _ = draw_drawer((100, 30), &mut a, halfway);
            assert!(a.detail_rect.x + a.detail_rect.width <= 100);
            assert!(a.list_rect.x + a.list_rect.width <= 100);

            let complete = start + Duration::from_millis(300);
            let final_text = draw_drawer((100, 30), &mut a, complete);
            let animated_final = a.detail_rect;
            assert!(animated_final.width > 0);
            assert!(final_text.contains(if worktrees {
                "WORKTREES"
            } else {
                "BEAD DETAILS"
            }));
            a.settings.animate_drawers = false;
            let static_text = draw_drawer((100, 30), &mut a, start);
            assert_eq!(a.detail_rect, animated_final);
            assert_eq!(static_text, final_text);
        }
    }

    #[test]
    fn visual_update_deadlines_cover_splash_drawer_and_toast_without_busy_polling() {
        let start = Instant::now();
        let mut a = app();
        assert_eq!(
            next_visual_update(&a, start, start + Duration::from_millis(299)),
            Some(start + Duration::from_millis(300))
        );
        assert_eq!(
            next_visual_update(&a, start, start + Duration::from_millis(300)),
            Some(start + Duration::from_millis(600))
        );
        assert_eq!(
            next_visual_update(&a, start, start + Duration::from_millis(600)),
            Some(start + Duration::from_millis(900))
        );
        assert_eq!(
            next_visual_update(&a, start, start + Duration::from_millis(900)),
            Some(start + SPLASH_DURATION)
        );
        assert_eq!(next_visual_update(&a, start, start + SPLASH_DURATION), None);
        assert_eq!(
            next_visual_update(&a, start, start + Duration::from_secs(2)),
            None
        );
        let animation_start = start + Duration::from_secs(3);
        a.drawer_animation = Some(crate::app::DrawerAnimation {
            worktrees: false,
            started: animation_start,
            from: 0.0,
            to: 1.0,
        });
        assert_eq!(
            next_visual_update(&a, start, animation_start),
            Some(animation_start + Duration::from_millis(35))
        );
        assert_eq!(
            next_visual_update(&a, start, animation_start + Duration::from_millis(35)),
            Some(animation_start + Duration::from_millis(70))
        );
        assert_eq!(
            next_visual_update(&a, start, animation_start + Duration::from_millis(210)),
            Some(animation_start + Duration::from_millis(220))
        );
        assert_eq!(
            next_visual_update(&a, start, animation_start + Duration::from_millis(220)),
            None
        );
        a.drawer_animation = None;
        let expiry = start + Duration::from_secs(5);
        a.toast = Some(crate::app::Toast {
            message: "temporary".into(),
            error: false,
            expires_at: Some(expiry),
        });
        assert_eq!(
            next_visual_update(&a, start, expiry - Duration::from_nanos(1)),
            Some(expiry)
        );
        assert_eq!(next_visual_update(&a, start, expiry), None);
    }
}
