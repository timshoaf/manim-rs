//! The manim CE named-color catalog.
//!
//! Every color from manim CE's `manim/utils/color/manim_colors.py` is available
//! two ways: as a module-level constant (re-exported at the crate root) and as
//! an associated constant on [`Color`].
//!
//! ```
//! use manim_color::{Color, BLUE_C};
//!
//! // Module-level constant (also `manim_color::BLUE_C`)…
//! assert_eq!(BLUE_C.to_hex(), "#58C4DD");
//! // …and the matching associated constant.
//! assert_eq!(Color::BLUE_C, BLUE_C);
//! ```
//!
//! The values are stored linear-light, precomputed from each color's sRGB hex
//! so the conversion needs no floating-point work at load time. `GREY`
//! spellings alias their `GRAY` counterparts, and the letter-suffix families
//! expose an unqualified alias at the `_C` shade (`BLUE == BLUE_C`), matching
//! manim.

use crate::Color;

/// Defines each catalog color as both a module-level `pub const` and an
/// associated `const` on [`Color`], with a generated doc example.
macro_rules! catalog {
    ($( $name:ident, $hex:literal, $color:expr );* $(;)?) => {
        $(
            #[doc = concat!("manim CE `", stringify!($name), "` — sRGB `", $hex, "`.")]
            #[doc = ""]
            #[doc = "```"]
            #[doc = concat!("assert_eq!(manim_color::", stringify!($name), ".to_hex(), \"", $hex, "\");")]
            #[doc = "```"]
            pub const $name: Color = $color;
        )*

        impl Color {
            $(
                #[doc = concat!("manim CE `", stringify!($name), "` — sRGB `", $hex, "`.")]
                #[doc = ""]
                #[doc = "```"]
                #[doc = concat!("assert_eq!(manim_color::Color::", stringify!($name), ".to_hex(), \"", $hex, "\");")]
                #[doc = "```"]
                pub const $name: Color = $color;
            )*
        }
    };
}

catalog! {
    // ---- canonical colors ----
    WHITE, "#FFFFFF", Color { r: 1.0, g: 1.0, b: 1.0, a: 1.0 };
    BLACK, "#000000", Color { r: 0.0, g: 0.0, b: 0.0, a: 1.0 };
    GRAY_A, "#DDDDDD", Color { r: 0.7230551, g: 0.7230551, b: 0.7230551, a: 1.0 };
    GRAY_B, "#BBBBBB", Color { r: 0.49693298, g: 0.49693298, b: 0.49693298, a: 1.0 };
    GRAY_C, "#888888", Color { r: 0.24620132, g: 0.24620132, b: 0.24620132, a: 1.0 };
    GRAY_D, "#444444", Color { r: 0.05780543, g: 0.05780543, b: 0.05780543, a: 1.0 };
    GRAY_E, "#222222", Color { r: 0.015996294, g: 0.015996294, b: 0.015996294, a: 1.0 };
    BLUE_A, "#C7E9F1", Color { r: 0.57112485, g: 0.8148466, b: 0.8796224, a: 1.0 };
    BLUE_B, "#9CDCEB", Color { r: 0.33245152, g: 0.7156935, b: 0.8307699, a: 1.0 };
    BLUE_C, "#58C4DD", Color { r: 0.09758735, g: 0.55201143, b: 0.7230551, a: 1.0 };
    BLUE_D, "#29ABCA", Color { r: 0.022173885, g: 0.4072402, b: 0.59061885, a: 1.0 };
    BLUE_E, "#236B8E", Color { r: 0.016807375, g: 0.14702727, b: 0.2704978, a: 1.0 };
    PURE_BLUE, "#0000FF", Color { r: 0.0, g: 0.0, b: 1.0, a: 1.0 };
    TEAL_A, "#ACEAD7", Color { r: 0.4125426, g: 0.82278574, b: 0.6795425, a: 1.0 };
    TEAL_B, "#76DDC0", Color { r: 0.18116425, g: 0.7230551, b: 0.5271151, a: 1.0 };
    TEAL_C, "#5CD0B3", Color { r: 0.107023105, g: 0.63075715, b: 0.4507858, a: 1.0 };
    TEAL_D, "#55C1A7", Color { r: 0.09084171, g: 0.5332764, b: 0.38642943, a: 1.0 };
    TEAL_E, "#49A88F", Color { r: 0.06662594, g: 0.39157248, b: 0.2746773, a: 1.0 };
    GREEN_A, "#C9E2AE", Color { r: 0.58407843, g: 0.7605245, b: 0.42326766, a: 1.0 };
    GREEN_B, "#A6CF8C", Color { r: 0.38132602, g: 0.6239604, b: 0.26225066, a: 1.0 };
    GREEN_C, "#83C167", Color { r: 0.22696587, g: 0.5332764, b: 0.13563333, a: 1.0 };
    GREEN_D, "#77B05D", Color { r: 0.18447499, g: 0.43415365, b: 0.10946171, a: 1.0 };
    GREEN_E, "#699C52", Color { r: 0.14126329, g: 0.33245152, b: 0.08437621, a: 1.0 };
    PURE_GREEN, "#00FF00", Color { r: 0.0, g: 1.0, b: 0.0, a: 1.0 };
    YELLOW_A, "#FFF1B6", Color { r: 1.0, g: 0.8796224, b: 0.4677838, a: 1.0 };
    YELLOW_B, "#FFEA94", Color { r: 1.0, g: 0.82278574, b: 0.29613826, a: 1.0 };
    YELLOW_C, "#FFFF00", Color { r: 1.0, g: 1.0, b: 0.0, a: 1.0 };
    YELLOW_D, "#F4D345", Color { r: 0.9046612, g: 0.65140563, b: 0.059511237, a: 1.0 };
    YELLOW_E, "#E8C11C", Color { r: 0.80695224, g: 0.5332764, b: 0.011612245, a: 1.0 };
    GOLD_A, "#F7C797", Color { r: 0.9301109, g: 0.57112485, b: 0.30946892, a: 1.0 };
    GOLD_B, "#F9B775", Color { r: 0.9473065, g: 0.47353148, b: 0.17788842, a: 1.0 };
    GOLD_C, "#F0AC5F", Color { r: 0.8713671, g: 0.4125426, b: 0.114435375, a: 1.0 };
    GOLD_D, "#E1A158", Color { r: 0.7529422, g: 0.35640013, b: 0.09758735, a: 1.0 };
    GOLD_E, "#C78D46", Color { r: 0.57112485, g: 0.2663556, b: 0.061246052, a: 1.0 };
    RED_A, "#F7A1A3", Color { r: 0.9301109, g: 0.35640013, b: 0.3662526, a: 1.0 };
    RED_B, "#FF8080", Color { r: 1.0, g: 0.2158605, b: 0.2158605, a: 1.0 };
    RED_C, "#FC6255", Color { r: 0.9734453, g: 0.122138776, b: 0.09084171, a: 1.0 };
    RED_D, "#E65A4C", Color { r: 0.7912979, g: 0.10224173, b: 0.07227185, a: 1.0 };
    RED_E, "#CF5044", Color { r: 0.6239604, g: 0.08021982, b: 0.05780543, a: 1.0 };
    PURE_RED, "#FF0000", Color { r: 1.0, g: 0.0, b: 0.0, a: 1.0 };
    MAROON_A, "#ECABC1", Color { r: 0.838799, g: 0.4072402, b: 0.5332764, a: 1.0 };
    MAROON_B, "#EC92AB", Color { r: 0.838799, g: 0.28744084, b: 0.4072402, a: 1.0 };
    MAROON_C, "#C55F73", Color { r: 0.5583404, g: 0.114435375, b: 0.17144111, a: 1.0 };
    MAROON_D, "#A24D61", Color { r: 0.3613068, g: 0.07421357, b: 0.11953843, a: 1.0 };
    MAROON_E, "#94424F", Color { r: 0.29613826, g: 0.054480277, b: 0.07818742, a: 1.0 };
    PURPLE_A, "#CAA3E8", Color { r: 0.59061885, g: 0.3662526, b: 0.80695224, a: 1.0 };
    PURPLE_B, "#B189C6", Color { r: 0.43965718, g: 0.25015828, b: 0.5647115, a: 1.0 };
    PURPLE_C, "#9A72AC", Color { r: 0.3231432, g: 0.1682694, b: 0.4125426, a: 1.0 };
    PURPLE_D, "#715582", Color { r: 0.1651322, g: 0.09084171, b: 0.22322796, a: 1.0 };
    PURPLE_E, "#644172", Color { r: 0.12743768, g: 0.052860647, b: 0.1682694, a: 1.0 };
    PINK, "#D147BD", Color { r: 0.63759685, g: 0.063010015, b: 0.50888133, a: 1.0 };
    LIGHT_PINK, "#DC75CD", Color { r: 0.7156935, g: 0.17788842, b: 0.61049557, a: 1.0 };
    ORANGE, "#FF862F", Color { r: 1.0, g: 0.23839757, b: 0.02842604, a: 1.0 };
    LIGHT_BROWN, "#CD853F", Color { r: 0.61049557, g: 0.23455058, b: 0.049706567, a: 1.0 };
    DARK_BROWN, "#8B4513", Color { r: 0.25818285, g: 0.059511237, b: 0.0065120906, a: 1.0 };
    GRAY_BROWN, "#736357", Color { r: 0.17144111, g: 0.12477182, b: 0.09530747, a: 1.0 };

    // ---- GREY spellings and family/shade aliases ----
    GREY_A, "#DDDDDD", GRAY_A;
    GREY_B, "#BBBBBB", GRAY_B;
    GREY_C, "#888888", GRAY_C;
    GREY_D, "#444444", GRAY_D;
    GREY_E, "#222222", GRAY_E;
    GRAY, "#888888", GRAY_C;
    GREY, "#888888", GRAY_C;
    LIGHTER_GRAY, "#DDDDDD", GRAY_A;
    LIGHTER_GREY, "#DDDDDD", GRAY_A;
    LIGHT_GRAY, "#BBBBBB", GRAY_B;
    LIGHT_GREY, "#BBBBBB", GRAY_B;
    DARK_GRAY, "#444444", GRAY_D;
    DARK_GREY, "#444444", GRAY_D;
    DARKER_GRAY, "#222222", GRAY_E;
    DARKER_GREY, "#222222", GRAY_E;
    GREY_BROWN, "#736357", GRAY_BROWN;
    BLUE, "#58C4DD", BLUE_C;
    TEAL, "#5CD0B3", TEAL_C;
    GREEN, "#83C167", GREEN_C;
    YELLOW, "#FFFF00", YELLOW_C;
    GOLD, "#F0AC5F", GOLD_C;
    RED, "#FC6255", RED_C;
    MAROON, "#C55F73", MAROON_C;
    PURPLE, "#9A72AC", PURPLE_C;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_hex_values() {
        assert_eq!(BLUE_C.to_hex(), "#58C4DD");
        assert_eq!(RED_C.to_hex(), "#FC6255");
        assert_eq!(WHITE.to_hex(), "#FFFFFF");
        assert_eq!(BLACK.to_hex(), "#000000");
        assert_eq!(GRAY_BROWN.to_hex(), "#736357");
        assert_eq!(MAROON_E.to_hex(), "#94424F");
    }

    #[test]
    fn grey_aliases_match_gray() {
        assert_eq!(GREY_A, GRAY_A);
        assert_eq!(GREY_BROWN, GRAY_BROWN);
        assert_eq!(LIGHT_GREY, LIGHT_GRAY);
        assert_eq!(DARKER_GREY, DARKER_GRAY);
    }

    #[test]
    fn family_shade_aliases() {
        assert_eq!(BLUE, BLUE_C);
        assert_eq!(GREEN, GREEN_C);
        assert_eq!(RED, RED_C);
        assert_eq!(PURPLE, PURPLE_C);
        assert_eq!(GRAY, GRAY_C);
    }

    #[test]
    fn associated_consts_match_module_consts() {
        assert_eq!(Color::BLUE_C, BLUE_C);
        assert_eq!(Color::WHITE, WHITE);
        assert_eq!(Color::GREY_BROWN, GRAY_BROWN);
    }

    #[test]
    fn pure_colors_are_extreme() {
        assert_eq!(PURE_RED.to_srgb_u8(), [255, 0, 0, 255]);
        assert_eq!(PURE_GREEN.to_srgb_u8(), [0, 255, 0, 255]);
        assert_eq!(PURE_BLUE.to_srgb_u8(), [0, 0, 255, 255]);
    }
}
