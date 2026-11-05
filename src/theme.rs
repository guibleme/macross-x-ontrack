use crate::settings::Theme;
use crate::worktrees::WorktreeStatus;
use ratatui::style::Color;

#[derive(Clone, Copy, Debug)]
pub struct Palette {
    pub background: Color,
    pub panel: Color,
    pub panel_alt: Color,
    pub foreground: Color,
    pub muted: Color,
    pub border: Color,
    pub accent: Color,
    pub accent_alt: Color,
    pub warning: Color,
    pub danger: Color,
    pub success: Color,
    pub info: Color,
    pub code_background: Color,
    pub selected_background: Color,
    pub selected_foreground: Color,
}

#[derive(Clone, Copy, Debug)]
pub struct ThemeDefinition {
    pub theme: Theme,
    pub id: &'static str,
    pub name: &'static str,
    pub is_light: bool,
    pub palette: Palette,
}

const fn rgb(red: u8, green: u8, blue: u8) -> Color {
    Color::Rgb(red, green, blue)
}

// Imported palettes and role/contrast adaptations: see docs/settings.md and assets/themes.
pub const CATALOG: [ThemeDefinition; 40] = [
    ThemeDefinition {
        theme: Theme::Dark,
        id: "dark",
        name: "MXO Dark",
        is_light: false,
        palette: Palette {
            background: rgb(13, 23, 28),
            panel: rgb(20, 33, 38),
            panel_alt: rgb(28, 43, 48),
            foreground: rgb(220, 228, 229),
            muted: rgb(123, 144, 148),
            border: rgb(73, 211, 182),
            accent: rgb(73, 211, 182),
            accent_alt: rgb(145, 170, 220),
            warning: rgb(239, 183, 99),
            danger: rgb(255, 123, 123),
            success: rgb(73, 211, 182),
            info: rgb(125, 168, 235),
            code_background: rgb(9, 17, 21),
            selected_background: rgb(73, 211, 182),
            selected_foreground: rgb(13, 23, 28),
        },
    },
    ThemeDefinition {
        theme: Theme::Light,
        id: "light",
        name: "MXO Light",
        is_light: true,
        palette: Palette {
            background: rgb(246, 248, 247),
            panel: rgb(232, 239, 237),
            panel_alt: rgb(222, 232, 229),
            foreground: rgb(32, 43, 45),
            muted: rgb(76, 98, 101),
            border: rgb(83, 101, 104),
            accent: rgb(0, 113, 94),
            accent_alt: rgb(55, 91, 142),
            warning: rgb(139, 86, 0),
            danger: rgb(150, 45, 45),
            success: rgb(0, 112, 91),
            info: rgb(0, 100, 112),
            code_background: rgb(238, 243, 241),
            selected_background: rgb(0, 113, 94),
            selected_foreground: rgb(246, 248, 247),
        },
    },
    ThemeDefinition {
        theme: Theme::TokyoNight,
        id: "tokyo-night",
        name: "Tokyo Night",
        is_light: false,
        palette: Palette {
            background: rgb(26, 27, 38),
            panel: rgb(36, 40, 59),
            panel_alt: rgb(41, 46, 66),
            foreground: rgb(192, 202, 245),
            muted: rgb(127, 137, 174),
            border: rgb(65, 72, 104),
            accent: rgb(122, 162, 247),
            accent_alt: rgb(187, 154, 247),
            warning: rgb(224, 175, 104),
            danger: rgb(247, 118, 142),
            success: rgb(158, 206, 106),
            info: rgb(125, 207, 255),
            code_background: rgb(31, 35, 53),
            selected_background: rgb(122, 162, 247),
            selected_foreground: rgb(26, 27, 38),
        },
    },
    ThemeDefinition {
        theme: Theme::Nord,
        id: "nord",
        name: "Nord",
        is_light: false,
        palette: Palette {
            background: rgb(46, 52, 64),
            panel: rgb(59, 66, 82),
            panel_alt: rgb(67, 76, 94),
            foreground: rgb(236, 239, 244),
            muted: rgb(177, 186, 201),
            border: rgb(76, 86, 106),
            accent: rgb(136, 192, 208),
            accent_alt: rgb(180, 142, 173),
            warning: rgb(235, 203, 139),
            danger: rgb(191, 97, 106),
            success: rgb(163, 190, 140),
            info: rgb(129, 161, 193),
            code_background: rgb(43, 49, 60),
            selected_background: rgb(136, 192, 208),
            selected_foreground: rgb(46, 52, 64),
        },
    },
    ThemeDefinition {
        theme: Theme::Dracula,
        id: "dracula",
        name: "Dracula",
        is_light: false,
        palette: Palette {
            background: rgb(40, 42, 54),
            panel: rgb(52, 55, 70),
            panel_alt: rgb(68, 71, 90),
            foreground: rgb(248, 248, 242),
            muted: rgb(189, 189, 204),
            border: rgb(98, 101, 128),
            accent: rgb(189, 147, 249),
            accent_alt: rgb(255, 121, 198),
            warning: rgb(241, 250, 140),
            danger: rgb(255, 85, 85),
            success: rgb(80, 250, 123),
            info: rgb(139, 233, 253),
            code_background: rgb(33, 34, 44),
            selected_background: rgb(189, 147, 249),
            selected_foreground: rgb(40, 42, 54),
        },
    },
    ThemeDefinition {
        theme: Theme::GruvboxDark,
        id: "gruvbox-dark",
        name: "Gruvbox Dark",
        is_light: false,
        palette: Palette {
            background: rgb(40, 40, 40),
            panel: rgb(60, 56, 54),
            panel_alt: rgb(80, 73, 69),
            foreground: rgb(235, 219, 178),
            muted: rgb(189, 174, 147),
            border: rgb(102, 92, 84),
            accent: rgb(250, 189, 47),
            accent_alt: rgb(211, 134, 155),
            warning: rgb(250, 189, 47),
            danger: rgb(251, 73, 52),
            success: rgb(184, 187, 38),
            info: rgb(131, 165, 152),
            code_background: rgb(29, 32, 33),
            selected_background: rgb(250, 189, 47),
            selected_foreground: rgb(40, 40, 40),
        },
    },
    ThemeDefinition {
        theme: Theme::GruvboxLight,
        id: "gruvbox-light",
        name: "Gruvbox Light",
        is_light: true,
        palette: Palette {
            background: rgb(251, 241, 199),
            panel: rgb(242, 229, 188),
            panel_alt: rgb(235, 219, 178),
            foreground: rgb(60, 56, 54),
            muted: rgb(102, 92, 84),
            border: rgb(168, 153, 132),
            accent: rgb(175, 58, 3),
            accent_alt: rgb(143, 63, 113),
            warning: rgb(181, 118, 20),
            danger: rgb(157, 0, 6),
            success: rgb(121, 116, 14),
            info: rgb(7, 102, 120),
            code_background: rgb(247, 236, 194),
            selected_background: rgb(175, 58, 3),
            selected_foreground: rgb(251, 241, 199),
        },
    },
    ThemeDefinition {
        theme: Theme::CatppuccinMocha,
        id: "catppuccin-mocha",
        name: "Catppuccin Mocha",
        is_light: false,
        palette: Palette {
            background: rgb(30, 30, 46),
            panel: rgb(49, 50, 68),
            panel_alt: rgb(69, 71, 90),
            foreground: rgb(205, 214, 244),
            muted: rgb(166, 173, 200),
            border: rgb(88, 91, 112),
            accent: rgb(203, 166, 247),
            accent_alt: rgb(245, 194, 231),
            warning: rgb(249, 226, 175),
            danger: rgb(243, 139, 168),
            success: rgb(166, 227, 161),
            info: rgb(137, 220, 235),
            code_background: rgb(24, 24, 37),
            selected_background: rgb(203, 166, 247),
            selected_foreground: rgb(30, 30, 46),
        },
    },
    ThemeDefinition {
        theme: Theme::CatppuccinLatte,
        id: "catppuccin-latte",
        name: "Catppuccin Latte",
        is_light: true,
        palette: Palette {
            background: rgb(239, 241, 245),
            panel: rgb(230, 233, 239),
            panel_alt: rgb(220, 224, 232),
            foreground: rgb(76, 79, 105),
            muted: rgb(91, 94, 115),
            border: rgb(156, 160, 176),
            accent: rgb(136, 57, 239),
            accent_alt: rgb(173, 50, 137),
            warning: rgb(150, 90, 0),
            danger: rgb(210, 15, 57),
            success: rgb(43, 110, 27),
            info: rgb(0, 110, 150),
            code_background: rgb(232, 235, 241),
            selected_background: rgb(136, 57, 239),
            selected_foreground: rgb(239, 241, 245),
        },
    },
    ThemeDefinition {
        theme: Theme::SolarizedDark,
        id: "solarized-dark",
        name: "Solarized Dark",
        is_light: false,
        palette: Palette {
            background: rgb(0, 43, 54),
            panel: rgb(7, 54, 66),
            panel_alt: rgb(19, 73, 83),
            foreground: rgb(147, 161, 161),
            muted: rgb(147, 161, 161),
            border: rgb(45, 105, 117),
            accent: rgb(42, 161, 152),
            accent_alt: rgb(108, 113, 196),
            warning: rgb(181, 137, 0),
            danger: rgb(220, 50, 47),
            success: rgb(133, 153, 0),
            info: rgb(80, 170, 230),
            code_background: rgb(0, 35, 44),
            selected_background: rgb(42, 161, 152),
            selected_foreground: rgb(0, 43, 54),
        },
    },
    ThemeDefinition {
        theme: Theme::SolarizedLight,
        id: "solarized-light",
        name: "Solarized Light",
        is_light: true,
        palette: Palette {
            background: rgb(253, 246, 227),
            panel: rgb(238, 232, 213),
            panel_alt: rgb(226, 218, 194),
            foreground: rgb(65, 85, 90),
            muted: rgb(65, 85, 90),
            border: rgb(147, 161, 161),
            accent: rgb(0, 90, 140),
            accent_alt: rgb(108, 113, 196),
            warning: rgb(125, 95, 0),
            danger: rgb(220, 50, 47),
            success: rgb(85, 100, 0),
            info: rgb(0, 105, 99),
            code_background: rgb(246, 239, 220),
            selected_background: rgb(0, 90, 140),
            selected_foreground: rgb(253, 246, 227),
        },
    },
    ThemeDefinition {
        theme: Theme::RosePine,
        id: "rose-pine",
        name: "Rosé Pine",
        is_light: false,
        palette: Palette {
            background: rgb(25, 23, 36),
            panel: rgb(38, 35, 58),
            panel_alt: rgb(47, 44, 69),
            foreground: rgb(224, 222, 244),
            muted: rgb(144, 140, 170),
            border: rgb(86, 82, 110),
            accent: rgb(196, 167, 231),
            accent_alt: rgb(235, 111, 146),
            warning: rgb(246, 193, 119),
            danger: rgb(235, 111, 146),
            success: rgb(156, 207, 216),
            info: rgb(49, 116, 143),
            code_background: rgb(31, 29, 46),
            selected_background: rgb(196, 167, 231),
            selected_foreground: rgb(25, 23, 36),
        },
    },
    ThemeDefinition {
        theme: Theme::FieldJournal,
        id: "field-journal",
        name: "Field Journal",
        is_light: false,
        palette: Palette {
            background: rgb(21, 29, 27),
            panel: rgb(31, 43, 39),
            panel_alt: rgb(31, 43, 39),
            foreground: rgb(225, 210, 165),
            muted: rgb(151, 164, 137),
            border: rgb(85, 105, 81),
            accent: rgb(229, 181, 82),
            accent_alt: rgb(196, 167, 231),
            warning: rgb(239, 183, 99),
            danger: rgb(206, 91, 77),
            success: rgb(112, 190, 113),
            info: rgb(103, 160, 208),
            code_background: rgb(21, 29, 27),
            selected_background: rgb(229, 181, 82),
            selected_foreground: rgb(21, 29, 27),
        },
    },
    ThemeDefinition {
        theme: Theme::OpenCode,
        id: "opencode",
        name: "OpenCode",
        is_light: false,
        palette: Palette {
            background: rgb(10, 10, 10),
            panel: rgb(20, 20, 20),
            panel_alt: rgb(20, 20, 20),
            foreground: rgb(238, 238, 238),
            muted: rgb(154, 154, 154),
            border: rgb(72, 72, 72),
            accent: rgb(250, 178, 131),
            accent_alt: rgb(196, 167, 231),
            warning: rgb(239, 183, 99),
            danger: rgb(224, 108, 117),
            success: rgb(127, 216, 143),
            info: rgb(86, 182, 194),
            code_background: rgb(10, 10, 10),
            selected_background: rgb(250, 178, 131),
            selected_foreground: rgb(10, 10, 10),
        },
    },
    ThemeDefinition {
        theme: Theme::Aura,
        id: "aura",
        name: "Aura",
        is_light: false,
        palette: Palette {
            background: rgb(15, 15, 15),
            panel: rgb(21, 20, 27),
            panel_alt: rgb(21, 20, 27),
            foreground: rgb(237, 236, 238),
            muted: rgb(126, 126, 126),
            border: rgb(45, 45, 45),
            accent: rgb(162, 119, 255),
            accent_alt: rgb(196, 167, 231),
            warning: rgb(239, 183, 99),
            danger: rgb(255, 103, 103),
            success: rgb(97, 255, 202),
            info: rgb(162, 119, 255),
            code_background: rgb(15, 15, 15),
            selected_background: rgb(162, 119, 255),
            selected_foreground: rgb(15, 15, 15),
        },
    },
    ThemeDefinition {
        theme: Theme::Ayu,
        id: "ayu",
        name: "Ayu",
        is_light: false,
        palette: Palette {
            background: rgb(11, 14, 20),
            panel: rgb(15, 19, 26),
            panel_alt: rgb(15, 19, 26),
            foreground: rgb(191, 189, 182),
            muted: rgb(120, 125, 136),
            border: rgb(108, 115, 128),
            accent: rgb(89, 194, 255),
            accent_alt: rgb(196, 167, 231),
            warning: rgb(239, 183, 99),
            danger: rgb(217, 87, 87),
            success: rgb(127, 217, 98),
            info: rgb(57, 186, 230),
            code_background: rgb(11, 14, 20),
            selected_background: rgb(89, 194, 255),
            selected_foreground: rgb(11, 14, 20),
        },
    },
    ThemeDefinition {
        theme: Theme::Carbonfox,
        id: "carbonfox",
        name: "Carbonfox",
        is_light: false,
        palette: Palette {
            background: rgb(22, 22, 22),
            panel: rgb(26, 26, 26),
            panel_alt: rgb(26, 26, 26),
            foreground: rgb(242, 244, 248),
            muted: rgb(125, 132, 143),
            border: rgb(48, 48, 48),
            accent: rgb(51, 177, 255),
            accent_alt: rgb(196, 167, 231),
            warning: rgb(239, 183, 99),
            danger: rgb(238, 83, 150),
            success: rgb(37, 190, 106),
            info: rgb(120, 169, 255),
            code_background: rgb(22, 22, 22),
            selected_background: rgb(51, 177, 255),
            selected_foreground: rgb(22, 22, 22),
        },
    },
    ThemeDefinition {
        theme: Theme::CatppuccinFrappe,
        id: "catppuccin-frappe",
        name: "Catppuccin Frappé",
        is_light: false,
        palette: Palette {
            background: rgb(48, 52, 70),
            panel: rgb(41, 44, 60),
            panel_alt: rgb(41, 44, 60),
            foreground: rgb(198, 208, 245),
            muted: rgb(148, 156, 184),
            border: rgb(65, 69, 89),
            accent: rgb(141, 164, 226),
            accent_alt: rgb(196, 167, 231),
            warning: rgb(239, 183, 99),
            danger: rgb(231, 130, 132),
            success: rgb(166, 209, 137),
            info: rgb(129, 200, 190),
            code_background: rgb(48, 52, 70),
            selected_background: rgb(141, 164, 226),
            selected_foreground: rgb(48, 52, 70),
        },
    },
    ThemeDefinition {
        theme: Theme::CatppuccinMacchiato,
        id: "catppuccin-macchiato",
        name: "Catppuccin Macchiato",
        is_light: false,
        palette: Palette {
            background: rgb(36, 39, 58),
            panel: rgb(30, 32, 48),
            panel_alt: rgb(30, 32, 48),
            foreground: rgb(202, 211, 245),
            muted: rgb(147, 154, 183),
            border: rgb(54, 58, 79),
            accent: rgb(138, 173, 244),
            accent_alt: rgb(196, 167, 231),
            warning: rgb(239, 183, 99),
            danger: rgb(237, 135, 150),
            success: rgb(166, 218, 149),
            info: rgb(139, 213, 202),
            code_background: rgb(36, 39, 58),
            selected_background: rgb(138, 173, 244),
            selected_foreground: rgb(36, 39, 58),
        },
    },
    ThemeDefinition {
        theme: Theme::Cobalt2,
        id: "cobalt2",
        name: "Cobalt2",
        is_light: false,
        palette: Palette {
            background: rgb(25, 53, 73),
            panel: rgb(18, 39, 56),
            panel_alt: rgb(18, 39, 56),
            foreground: rgb(255, 255, 255),
            muted: rgb(173, 183, 201),
            border: rgb(31, 70, 98),
            accent: rgb(23, 159, 255),
            accent_alt: rgb(196, 167, 231),
            warning: rgb(239, 183, 99),
            danger: rgb(255, 0, 136),
            success: rgb(158, 255, 128),
            info: rgb(255, 157, 0),
            code_background: rgb(25, 53, 73),
            selected_background: rgb(23, 159, 255),
            selected_foreground: rgb(25, 53, 73),
        },
    },
    ThemeDefinition {
        theme: Theme::Cursor,
        id: "cursor",
        name: "Cursor",
        is_light: false,
        palette: Palette {
            background: rgb(24, 24, 24),
            panel: rgb(20, 20, 20),
            panel_alt: rgb(20, 20, 20),
            foreground: rgb(228, 228, 228),
            muted: rgb(129, 129, 129),
            border: rgb(35, 35, 35),
            accent: rgb(136, 192, 208),
            accent_alt: rgb(196, 167, 231),
            warning: rgb(239, 183, 99),
            danger: rgb(227, 70, 113),
            success: rgb(63, 162, 102),
            info: rgb(129, 161, 193),
            code_background: rgb(24, 24, 24),
            selected_background: rgb(136, 192, 208),
            selected_foreground: rgb(24, 24, 24),
        },
    },
    ThemeDefinition {
        theme: Theme::Everforest,
        id: "everforest",
        name: "Everforest",
        is_light: false,
        palette: Palette {
            background: rgb(45, 53, 59),
            panel: rgb(51, 60, 67),
            panel_alt: rgb(51, 60, 67),
            foreground: rgb(211, 198, 170),
            muted: rgb(157, 167, 155),
            border: rgb(133, 146, 137),
            accent: rgb(167, 192, 128),
            accent_alt: rgb(196, 167, 231),
            warning: rgb(239, 183, 99),
            danger: rgb(230, 126, 128),
            success: rgb(167, 192, 128),
            info: rgb(131, 192, 146),
            code_background: rgb(45, 53, 59),
            selected_background: rgb(167, 192, 128),
            selected_foreground: rgb(45, 53, 59),
        },
    },
    ThemeDefinition {
        theme: Theme::Flexoki,
        id: "flexoki",
        name: "Flexoki",
        is_light: false,
        palette: Palette {
            background: rgb(16, 15, 15),
            panel: rgb(28, 27, 26),
            panel_alt: rgb(28, 27, 26),
            foreground: rgb(206, 205, 195),
            muted: rgb(132, 131, 126),
            border: rgb(87, 86, 83),
            accent: rgb(218, 112, 44),
            accent_alt: rgb(196, 167, 231),
            warning: rgb(239, 183, 99),
            danger: rgb(209, 77, 65),
            success: rgb(135, 154, 57),
            info: rgb(58, 169, 159),
            code_background: rgb(16, 15, 15),
            selected_background: rgb(218, 112, 44),
            selected_foreground: rgb(16, 15, 15),
        },
    },
    ThemeDefinition {
        theme: Theme::GitHub,
        id: "github",
        name: "GitHub",
        is_light: false,
        palette: Palette {
            background: rgb(13, 17, 23),
            panel: rgb(1, 4, 9),
            panel_alt: rgb(1, 4, 9),
            foreground: rgb(201, 209, 217),
            muted: rgb(139, 148, 158),
            border: rgb(48, 54, 61),
            accent: rgb(88, 166, 255),
            accent_alt: rgb(196, 167, 231),
            warning: rgb(239, 183, 99),
            danger: rgb(248, 81, 73),
            success: rgb(63, 185, 80),
            info: rgb(210, 153, 34),
            code_background: rgb(13, 17, 23),
            selected_background: rgb(88, 166, 255),
            selected_foreground: rgb(13, 17, 23),
        },
    },
    ThemeDefinition {
        theme: Theme::Kanagawa,
        id: "kanagawa",
        name: "Kanagawa",
        is_light: false,
        palette: Palette {
            background: rgb(31, 31, 40),
            panel: rgb(42, 42, 55),
            panel_alt: rgb(42, 42, 55),
            foreground: rgb(220, 215, 186),
            muted: rgb(147, 146, 138),
            border: rgb(84, 84, 109),
            accent: rgb(126, 156, 216),
            accent_alt: rgb(196, 167, 231),
            warning: rgb(239, 183, 99),
            danger: rgb(232, 36, 36),
            success: rgb(152, 187, 108),
            info: rgb(118, 148, 106),
            code_background: rgb(31, 31, 40),
            selected_background: rgb(126, 156, 216),
            selected_foreground: rgb(31, 31, 40),
        },
    },
    ThemeDefinition {
        theme: Theme::LucentOrng,
        id: "lucent-orng",
        name: "Lucent Orng",
        is_light: false,
        palette: Palette {
            background: Color::Reset,
            panel: Color::Reset,
            panel_alt: Color::Reset,
            foreground: rgb(238, 238, 238),
            muted: rgb(128, 128, 128),
            border: rgb(236, 91, 43),
            accent: rgb(236, 91, 43),
            accent_alt: rgb(196, 167, 231),
            warning: rgb(239, 183, 99),
            danger: rgb(224, 108, 117),
            success: rgb(107, 161, 230),
            info: rgb(86, 182, 194),
            code_background: Color::Reset,
            selected_background: rgb(236, 91, 43),
            selected_foreground: rgb(10, 10, 10),
        },
    },
    ThemeDefinition {
        theme: Theme::Material,
        id: "material",
        name: "Material",
        is_light: false,
        palette: Palette {
            background: rgb(38, 50, 56),
            panel: rgb(30, 39, 44),
            panel_alt: rgb(30, 39, 44),
            foreground: rgb(238, 255, 255),
            muted: rgb(130, 156, 168),
            border: rgb(55, 71, 79),
            accent: rgb(130, 170, 255),
            accent_alt: rgb(196, 167, 231),
            warning: rgb(239, 183, 99),
            danger: rgb(240, 113, 120),
            success: rgb(195, 232, 141),
            info: rgb(255, 203, 107),
            code_background: rgb(38, 50, 56),
            selected_background: rgb(130, 170, 255),
            selected_foreground: rgb(38, 50, 56),
        },
    },
    ThemeDefinition {
        theme: Theme::Matrix,
        id: "matrix",
        name: "Matrix",
        is_light: false,
        palette: Palette {
            background: rgb(10, 14, 10),
            panel: rgb(14, 19, 13),
            panel_alt: rgb(14, 19, 13),
            foreground: rgb(98, 255, 148),
            muted: rgb(140, 163, 145),
            border: rgb(30, 42, 27),
            accent: rgb(46, 255, 106),
            accent_alt: rgb(196, 167, 231),
            warning: rgb(239, 183, 99),
            danger: rgb(255, 75, 75),
            success: rgb(98, 255, 148),
            info: rgb(48, 179, 255),
            code_background: rgb(10, 14, 10),
            selected_background: rgb(46, 255, 106),
            selected_foreground: rgb(10, 14, 10),
        },
    },
    ThemeDefinition {
        theme: Theme::Mercury,
        id: "mercury",
        name: "Mercury",
        is_light: false,
        palette: Palette {
            background: rgb(23, 23, 33),
            panel: rgb(16, 16, 26),
            panel_alt: rgb(16, 16, 26),
            foreground: rgb(221, 221, 229),
            muted: rgb(157, 157, 168),
            border: rgb(36, 36, 47),
            accent: rgb(141, 164, 245),
            accent_alt: rgb(196, 167, 231),
            warning: rgb(239, 183, 99),
            danger: rgb(252, 146, 180),
            success: rgb(119, 197, 153),
            info: rgb(119, 190, 207),
            code_background: rgb(23, 23, 33),
            selected_background: rgb(141, 164, 245),
            selected_foreground: rgb(23, 23, 33),
        },
    },
    ThemeDefinition {
        theme: Theme::Monokai,
        id: "monokai",
        name: "Monokai",
        is_light: false,
        palette: Palette {
            background: rgb(39, 40, 34),
            panel: rgb(30, 31, 28),
            panel_alt: rgb(30, 31, 28),
            foreground: rgb(248, 248, 242),
            muted: rgb(146, 142, 123),
            border: rgb(62, 61, 50),
            accent: rgb(102, 217, 239),
            accent_alt: rgb(196, 167, 231),
            warning: rgb(239, 183, 99),
            danger: rgb(249, 38, 114),
            success: rgb(166, 226, 46),
            info: rgb(253, 151, 31),
            code_background: rgb(39, 40, 34),
            selected_background: rgb(102, 217, 239),
            selected_foreground: rgb(39, 40, 34),
        },
    },
    ThemeDefinition {
        theme: Theme::NightOwl,
        id: "night-owl",
        name: "Night Owl",
        is_light: false,
        palette: Palette {
            background: rgb(1, 22, 39),
            panel: rgb(11, 37, 58),
            panel_alt: rgb(11, 37, 58),
            foreground: rgb(214, 222, 235),
            muted: rgb(111, 142, 167),
            border: rgb(95, 126, 151),
            accent: rgb(130, 170, 255),
            accent_alt: rgb(196, 167, 231),
            warning: rgb(239, 183, 99),
            danger: rgb(239, 83, 80),
            success: rgb(197, 228, 120),
            info: rgb(130, 170, 255),
            code_background: rgb(1, 22, 39),
            selected_background: rgb(130, 170, 255),
            selected_foreground: rgb(1, 22, 39),
        },
    },
    ThemeDefinition {
        theme: Theme::OneDark,
        id: "one-dark",
        name: "One Dark",
        is_light: false,
        palette: Palette {
            background: rgb(40, 44, 52),
            panel: rgb(33, 37, 43),
            panel_alt: rgb(33, 37, 43),
            foreground: rgb(171, 178, 191),
            muted: rgb(140, 147, 160),
            border: rgb(57, 63, 74),
            accent: rgb(97, 175, 239),
            accent_alt: rgb(196, 167, 231),
            warning: rgb(239, 183, 99),
            danger: rgb(224, 108, 117),
            success: rgb(152, 195, 121),
            info: rgb(209, 154, 102),
            code_background: rgb(40, 44, 52),
            selected_background: rgb(97, 175, 239),
            selected_foreground: rgb(40, 44, 52),
        },
    },
    ThemeDefinition {
        theme: Theme::Orng,
        id: "orng",
        name: "Orng",
        is_light: false,
        palette: Palette {
            background: rgb(10, 10, 10),
            panel: rgb(20, 20, 20),
            panel_alt: rgb(20, 20, 20),
            foreground: rgb(238, 238, 238),
            muted: rgb(128, 128, 128),
            border: rgb(236, 91, 43),
            accent: rgb(236, 91, 43),
            accent_alt: rgb(196, 167, 231),
            warning: rgb(239, 183, 99),
            danger: rgb(224, 108, 117),
            success: rgb(107, 161, 230),
            info: rgb(86, 182, 194),
            code_background: rgb(10, 10, 10),
            selected_background: rgb(236, 91, 43),
            selected_foreground: rgb(10, 10, 10),
        },
    },
    ThemeDefinition {
        theme: Theme::OsakaJade,
        id: "osaka-jade",
        name: "Osaka Jade",
        is_light: false,
        palette: Palette {
            background: rgb(17, 28, 24),
            panel: rgb(26, 37, 32),
            panel_alt: rgb(26, 37, 32),
            foreground: rgb(193, 196, 151),
            muted: rgb(121, 142, 129),
            border: rgb(61, 74, 68),
            accent: rgb(45, 213, 183),
            accent_alt: rgb(196, 167, 231),
            warning: rgb(239, 183, 99),
            danger: rgb(255, 83, 69),
            success: rgb(84, 158, 106),
            info: rgb(45, 213, 183),
            code_background: rgb(17, 28, 24),
            selected_background: rgb(45, 213, 183),
            selected_foreground: rgb(17, 28, 24),
        },
    },
    ThemeDefinition {
        theme: Theme::Palenight,
        id: "palenight",
        name: "Palenight",
        is_light: false,
        palette: Palette {
            background: rgb(41, 45, 62),
            panel: rgb(30, 33, 50),
            panel_alt: rgb(30, 33, 50),
            foreground: rgb(166, 172, 205),
            muted: rgb(140, 147, 186),
            border: rgb(50, 54, 74),
            accent: rgb(130, 170, 255),
            accent_alt: rgb(196, 167, 231),
            warning: rgb(239, 183, 99),
            danger: rgb(240, 113, 120),
            success: rgb(195, 232, 141),
            info: rgb(247, 140, 108),
            code_background: rgb(41, 45, 62),
            selected_background: rgb(130, 170, 255),
            selected_foreground: rgb(41, 45, 62),
        },
    },
    ThemeDefinition {
        theme: Theme::Synthwave84,
        id: "synthwave-84",
        name: "Synthwave ’84",
        is_light: false,
        palette: Palette {
            background: rgb(38, 35, 53),
            panel: rgb(30, 26, 41),
            panel_alt: rgb(30, 26, 41),
            foreground: rgb(255, 255, 255),
            muted: rgb(132, 139, 189),
            border: rgb(73, 84, 149),
            accent: rgb(54, 249, 246),
            accent_alt: rgb(196, 167, 231),
            warning: rgb(239, 183, 99),
            danger: rgb(254, 68, 80),
            success: rgb(114, 241, 184),
            info: rgb(255, 139, 57),
            code_background: rgb(38, 35, 53),
            selected_background: rgb(54, 249, 246),
            selected_foreground: rgb(38, 35, 53),
        },
    },
    ThemeDefinition {
        theme: Theme::Vercel,
        id: "vercel",
        name: "Vercel",
        is_light: false,
        palette: Palette {
            background: rgb(0, 0, 0),
            panel: rgb(26, 26, 26),
            panel_alt: rgb(26, 26, 26),
            foreground: rgb(237, 237, 237),
            muted: rgb(135, 135, 135),
            border: rgb(31, 31, 31),
            accent: rgb(14, 126, 255),
            accent_alt: rgb(196, 167, 231),
            warning: rgb(239, 183, 99),
            danger: rgb(229, 72, 77),
            success: rgb(70, 167, 88),
            info: rgb(82, 168, 255),
            code_background: rgb(0, 0, 0),
            selected_background: rgb(14, 126, 255),
            selected_foreground: rgb(0, 0, 0),
        },
    },
    ThemeDefinition {
        theme: Theme::Vesper,
        id: "vesper",
        name: "Vesper",
        is_light: false,
        palette: Palette {
            background: rgb(16, 16, 16),
            panel: rgb(16, 16, 16),
            panel_alt: rgb(16, 16, 16),
            foreground: rgb(255, 255, 255),
            muted: rgb(160, 160, 160),
            border: rgb(40, 40, 40),
            accent: rgb(255, 199, 153),
            accent_alt: rgb(196, 167, 231),
            warning: rgb(239, 183, 99),
            danger: rgb(255, 128, 128),
            success: rgb(153, 255, 228),
            info: rgb(255, 199, 153),
            code_background: rgb(16, 16, 16),
            selected_background: rgb(255, 199, 153),
            selected_foreground: rgb(16, 16, 16),
        },
    },
    ThemeDefinition {
        theme: Theme::Zenburn,
        id: "zenburn",
        name: "Zenburn",
        is_light: false,
        palette: Palette {
            background: rgb(63, 63, 63),
            panel: rgb(79, 79, 79),
            panel_alt: rgb(79, 79, 79),
            foreground: rgb(220, 220, 204),
            muted: rgb(192, 192, 192),
            border: rgb(95, 95, 95),
            accent: rgb(140, 208, 211),
            accent_alt: rgb(196, 167, 231),
            warning: rgb(239, 183, 99),
            danger: rgb(204, 147, 147),
            success: rgb(133, 165, 133),
            info: rgb(223, 175, 143),
            code_background: rgb(63, 63, 63),
            selected_background: rgb(140, 208, 211),
            selected_foreground: rgb(63, 63, 63),
        },
    },
    ThemeDefinition {
        theme: Theme::System,
        id: "system",
        name: "System",
        is_light: false,
        palette: Palette {
            background: Color::Reset,
            panel: Color::Reset,
            panel_alt: Color::Reset,
            foreground: Color::Reset,
            muted: Color::Gray,
            border: Color::DarkGray,
            accent: Color::Cyan,
            accent_alt: Color::Magenta,
            warning: Color::Yellow,
            danger: Color::Red,
            success: Color::Green,
            info: Color::Blue,
            code_background: Color::Reset,
            selected_background: Color::Cyan,
            selected_foreground: Color::Black,
        },
    },
];

pub fn definition(theme: Theme) -> &'static ThemeDefinition {
    CATALOG
        .iter()
        .find(|entry| entry.theme == theme)
        .expect("each Theme variant must have one catalog entry")
}

pub fn find(id: &str) -> Option<Theme> {
    CATALOG
        .iter()
        .find(|entry| entry.id == id)
        .map(|entry| entry.theme)
}

pub fn status_color(theme: Theme, status: &str) -> Color {
    let definition = definition(theme);
    let p = definition.palette;
    match status {
        "open" => p.success,
        "ready" => {
            if definition.is_light {
                rgb(0, 91, 172)
            } else {
                rgb(125, 168, 235)
            }
        }
        "closed" | "blocked" => p.danger,
        "in_progress" => p.warning,
        "ready_to_merge" => {
            if theme == Theme::SolarizedDark {
                rgb(180, 125, 175)
            } else {
                p.accent_alt
            }
        }
        "deferred" => p.info,
        "unknown" | "" => p.muted,
        _ => p.accent_alt,
    }
}

pub fn worktree_status_color(theme: Theme, status: WorktreeStatus) -> Color {
    let p = definition(theme).palette;
    match status {
        WorktreeStatus::Reserved => p.info,
        WorktreeStatus::InProgress => p.warning,
        WorktreeStatus::Blocked => p.danger,
        WorktreeStatus::Unmerged => p.info,
        WorktreeStatus::MergeQueued => p.info,
        WorktreeStatus::MergeRequested | WorktreeStatus::MergeInProgress => p.warning,
        WorktreeStatus::NeedsResolution | WorktreeStatus::NeedsAttention => p.danger,
        WorktreeStatus::Integrated => p.success,
        WorktreeStatus::WorkspaceClosed => p.accent_alt,
        WorktreeStatus::CheckoutRemoved => p.muted,
        WorktreeStatus::DirtyCheckout => p.warning,
    }
}

pub fn issue_type_color(theme: Theme, kind: &str) -> Color {
    let p = definition(theme).palette;
    match kind.to_ascii_lowercase().as_str() {
        "task" => p.success,
        "bug" => p.danger,
        "feature" | "story" => status_color(theme, "ready"),
        "epic" | "decision" => p.accent_alt,
        "chore" | "unknown" => p.muted,
        "spike" => p.warning,
        "milestone" => p.accent,
        _ => p.muted,
    }
}

pub fn priority_color(theme: Theme, priority: u64) -> Color {
    let p = definition(theme).palette;
    match priority {
        0 => p.danger,
        1 | 2 => p.warning,
        3 => p.success,
        4 => p.muted,
        _ => p.muted,
    }
}

/// Re-map the legacy markdown renderer's semantic colors into the active theme.
pub fn markdown_foreground(theme: Theme, color: Color) -> Color {
    let p = definition(theme).palette;
    match color {
        Color::Reset | Color::White | Color::Rgb(220, 228, 229) => p.foreground,
        Color::Black | Color::Rgb(13, 23, 28) => p.background,
        Color::DarkGray | Color::Gray | Color::Rgb(123, 144, 148) => p.muted,
        Color::Rgb(73, 211, 182) => p.accent,
        Color::Rgb(239, 183, 99) => p.warning,
        Color::Rgb(255, 123, 123) => p.danger,
        Color::Indexed(1 | 9) => p.danger,
        Color::Indexed(2 | 10) => p.success,
        Color::Indexed(3 | 11) => p.warning,
        Color::Indexed(4 | 12) => p.info,
        Color::Indexed(5 | 13) => p.accent_alt,
        Color::Indexed(6 | 14) => p.accent,
        Color::Indexed(_) => p.muted,
        other => other,
    }
}

pub fn markdown_background(theme: Theme, color: Color) -> Color {
    let p = definition(theme).palette;
    match color {
        Color::Reset => p.background,
        Color::Black | Color::DarkGray | Color::Rgb(13, 23, 28) => p.code_background,
        Color::Rgb(232, 239, 237) => p.panel,
        other => markdown_foreground(theme, other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn luminance(color: Color) -> f64 {
        let Color::Rgb(r, g, b) = color else {
            panic!("theme palette should use RGB colors")
        };
        let linear = |value: u8| {
            let value = f64::from(value) / 255.0;
            if value <= 0.04045 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b)
    }

    fn contrast(left: Color, right: Color) -> f64 {
        let (lighter, darker) = {
            let left = luminance(left);
            let right = luminance(right);
            if left > right {
                (left, right)
            } else {
                (right, left)
            }
        };
        (lighter + 0.05) / (darker + 0.05)
    }

    #[test]
    fn catalog_ids_are_stable_unique_and_preserve_legacy_names() {
        let mut ids = CATALOG.iter().map(|entry| entry.id).collect::<Vec<_>>();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), CATALOG.len());
        assert_eq!(
            Theme::ALL,
            CATALOG.map(|entry| entry.theme),
            "Theme::ALL must track the catalog exactly"
        );
        assert_eq!(find("dark"), Some(Theme::Dark));
        assert_eq!(find("light"), Some(Theme::Light));
        for entry in CATALOG {
            assert!(
                entry
                    .id
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'),
                "theme ID must be lowercase kebab-case: {}",
                entry.id
            );
            assert_eq!(find(entry.id), Some(entry.theme));
        }
    }

    #[test]
    fn imported_catalog_includes_all_missing_mmogame_choices() {
        for id in [
            "field-journal",
            "opencode",
            "aura",
            "ayu",
            "carbonfox",
            "catppuccin-frappe",
            "catppuccin-macchiato",
            "cobalt2",
            "cursor",
            "everforest",
            "flexoki",
            "github",
            "kanagawa",
            "lucent-orng",
            "material",
            "matrix",
            "mercury",
            "monokai",
            "night-owl",
            "one-dark",
            "orng",
            "osaka-jade",
            "palenight",
            "synthwave-84",
            "vercel",
            "vesper",
            "zenburn",
            "system",
        ] {
            assert!(find(id).is_some(), "missing imported theme {id}");
        }
    }

    #[test]
    fn terminal_default_themes_preserve_surfaces_and_visible_selection() {
        for theme in [Theme::LucentOrng, Theme::System] {
            let p = definition(theme).palette;
            for surface in [p.background, p.panel, p.panel_alt, p.code_background] {
                assert_eq!(surface, Color::Reset);
            }
            assert_ne!(p.selected_background, Color::Reset);
            assert_ne!(p.selected_foreground, p.selected_background);
        }
        let p = definition(Theme::System).palette;
        assert_eq!(p.foreground, Color::Reset);
        assert_eq!(p.success, Color::Green);
        assert_eq!(p.danger, Color::Red);
        assert_eq!(p.warning, Color::Yellow);
    }

    #[test]
    fn every_palette_has_readable_text_selection_and_semantic_accents() {
        for entry in CATALOG {
            let p = entry.palette;
            // Terminal-default surfaces have no known RGB value to measure.
            if p.background == Color::Reset {
                continue;
            }
            assert!(
                contrast(p.foreground, p.background) >= 4.5,
                "{} foreground contrast too low",
                entry.id
            );
            assert!(
                contrast(p.muted, p.background) >= 4.5,
                "{} muted contrast too low",
                entry.id
            );
            assert!(
                contrast(p.foreground, p.panel) >= 4.5,
                "{} panel text contrast too low",
                entry.id
            );
            assert!(
                contrast(p.selected_foreground, p.selected_background) >= 4.5,
                "{} selected text contrast too low",
                entry.id
            );
            for (role, color) in [
                ("accent", p.accent),
                ("secondary accent", p.accent_alt),
                ("warning", p.warning),
                ("danger", p.danger),
                ("success", p.success),
                ("info", p.info),
            ] {
                assert!(
                    contrast(color, p.background) >= 3.0,
                    "{} {role} contrast too low",
                    entry.id
                );
            }
        }
    }

    #[test]
    fn ready_is_readable_blue_in_every_theme() {
        for entry in CATALOG {
            let ready = status_color(entry.theme, "ready");
            let Color::Rgb(red, green, blue) = ready else {
                panic!("Ready must have an RGB colour");
            };
            assert!(
                blue > green && green > red,
                "{} Ready is not blue",
                entry.id
            );
            assert_ne!(ready, status_color(entry.theme, "open"));
            if entry.palette.background == Color::Reset {
                continue;
            }
            assert!(
                contrast(ready, entry.palette.background) >= 3.0,
                "{} Ready has insufficient background contrast",
                entry.id
            );
            assert!(
                contrast(ready, entry.palette.panel) >= 3.0,
                "{} Ready has insufficient panel contrast",
                entry.id
            );
            assert_ne!(ready, status_color(entry.theme, "open"));
        }
    }

    #[test]
    fn issue_type_colors_keep_bug_red_feature_blue_and_safe_fallbacks() {
        for entry in CATALOG {
            assert_eq!(issue_type_color(entry.theme, "bug"), entry.palette.danger);
            assert_eq!(
                issue_type_color(entry.theme, "feature"),
                status_color(entry.theme, "ready")
            );
            assert_eq!(
                issue_type_color(entry.theme, "missing"),
                entry.palette.muted
            );
            let Color::Rgb(red, green, blue) = issue_type_color(entry.theme, "feature") else {
                panic!("feature must have an RGB color")
            };
            assert!(
                blue > green && green > red,
                "{} feature is not blue",
                entry.id
            );
        }
    }

    #[test]
    fn ready_to_merge_has_readable_distinct_semantic_color() {
        for entry in CATALOG {
            let color = status_color(entry.theme, "ready_to_merge");
            assert_ne!(
                color,
                status_color(entry.theme, "in_progress"),
                "{}",
                entry.id
            );
            assert_ne!(color, status_color(entry.theme, "ready"), "{}", entry.id);
            if entry.palette.background == Color::Reset {
                continue;
            }
            assert!(
                contrast(color, entry.palette.background) >= 3.0,
                "{}",
                entry.id
            );
            assert!(contrast(color, entry.palette.panel) >= 3.0, "{}", entry.id);
        }
    }

    #[test]
    fn worktree_statuses_keep_pending_and_attention_distinct_from_integration() {
        for theme in Theme::ALL {
            let color = |status| worktree_status_color(theme, status);
            assert_ne!(
                color(WorktreeStatus::MergeRequested),
                color(WorktreeStatus::Integrated)
            );
            assert_ne!(
                color(WorktreeStatus::NeedsResolution),
                color(WorktreeStatus::Integrated)
            );
            assert_ne!(
                color(WorktreeStatus::NeedsAttention),
                color(WorktreeStatus::Integrated)
            );
            for status in [
                WorktreeStatus::InProgress,
                WorktreeStatus::Blocked,
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
            ] {
                if theme != Theme::System {
                    assert!(matches!(color(status), Color::Rgb(_, _, _)));
                }
            }
        }
    }
}
