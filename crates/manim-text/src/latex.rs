//! A table-driven translator from a LaTeX-math subset to typst math syntax.
//!
//! This is the foundation of [`MathTex`](crate::MathTex): it turns the common
//! manim LaTeX corpus (`\frac`, `\sqrt`, `^`/`_`, greek, big operators,
//! relations, matrices, …) into an equivalent typst math string, which typst
//! then lays out. It is pure string→string, so it is exhaustively and cheaply
//! unit-tested without invoking typst.

use std::fmt;

/// An error translating LaTeX math to typst.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum MathError {
    /// A `\command` with no known translation.
    UnknownCommand(String),
    /// Unbalanced `{` / `}`.
    UnbalancedBraces,
    /// The input ended in the middle of a construct.
    UnexpectedEnd,
    /// A `\begin{env}` without a matching `\end{env}`.
    UnclosedEnvironment(String),
    /// A construct required an argument that was missing.
    MissingArgument(&'static str),
    /// typst failed to compile the translated source (diagnostics).
    Typeset(String),
}

impl fmt::Display for MathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MathError::UnknownCommand(c) => write!(f, "unknown LaTeX command: \\{c}"),
            MathError::UnbalancedBraces => write!(f, "unbalanced braces"),
            MathError::UnexpectedEnd => write!(f, "unexpected end of math input"),
            MathError::UnclosedEnvironment(e) => write!(f, "unclosed environment: {e}"),
            MathError::MissingArgument(what) => write!(f, "missing argument for {what}"),
            MathError::Typeset(msg) => write!(f, "typst typesetting failed: {msg}"),
        }
    }
}

impl std::error::Error for MathError {}

/// One LaTeX token.
#[derive(Debug, Clone, PartialEq)]
enum Tok {
    /// A `\command` (letters) or a `\x` escape (single non-letter).
    Ctrl(String),
    /// A literal character.
    Ch(char),
    /// `{`
    LBrace,
    /// `}`
    RBrace,
    /// `^`
    Sup,
    /// `_`
    Sub,
    /// `&`
    Amp,
}

/// Tokenizes a LaTeX-math string.
fn tokenize(src: &str) -> Vec<Tok> {
    let mut toks = Vec::new();
    let mut chars = src.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                // Command: letters, or a single non-letter escape.
                if matches!(chars.peek(), Some(d) if d.is_ascii_alphabetic()) {
                    let mut name = String::new();
                    while let Some(d) = chars.peek() {
                        if d.is_ascii_alphabetic() {
                            name.push(*d);
                            chars.next();
                        } else {
                            break;
                        }
                    }
                    toks.push(Tok::Ctrl(name));
                } else if let Some(d) = chars.next() {
                    toks.push(Tok::Ctrl(d.to_string()));
                }
            }
            '{' => toks.push(Tok::LBrace),
            '}' => toks.push(Tok::RBrace),
            '^' => toks.push(Tok::Sup),
            '_' => toks.push(Tok::Sub),
            '&' => toks.push(Tok::Amp),
            _ => toks.push(Tok::Ch(c)),
        }
    }
    toks
}

/// Translates a LaTeX-math string to typst math syntax.
///
/// ```
/// use manim_text::latex::translate;
/// assert_eq!(translate(r"\frac{a}{b}").unwrap(), "frac(a, b)");
/// assert_eq!(translate(r"x^2 + y^2").unwrap(), "x ^(2) + y ^(2)");
/// assert!(translate(r"\nope").is_err());
/// ```
pub fn translate(src: &str) -> Result<String, MathError> {
    let toks = tokenize(src);
    let mut pos = 0;
    let out = translate_seq(&toks, &mut pos, false)?;
    if pos != toks.len() {
        return Err(MathError::UnbalancedBraces);
    }
    Ok(normalize(&out))
}

/// Collapses whitespace runs to a single space (outside `"..."` strings) and
/// trims, so the translated output is deterministic regardless of the input's
/// spacing. typst ignores extra math whitespace, so this is safe.
fn normalize(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_quote = false;
    let mut prev_space = false;
    for c in s.chars() {
        if c == '"' {
            in_quote = !in_quote;
            out.push(c);
            prev_space = false;
        } else if !in_quote && c.is_whitespace() {
            if !prev_space && !out.is_empty() {
                out.push(' ');
                prev_space = true;
            }
        } else {
            out.push(c);
            prev_space = false;
        }
    }
    while out.ends_with(' ') {
        out.pop();
    }
    out
}

/// Translates tokens until the end (or, when `stop_at_brace`, until the matching
/// `}` which is left for the caller to consume).
fn translate_seq(toks: &[Tok], pos: &mut usize, stop_at_brace: bool) -> Result<String, MathError> {
    let mut out = String::new();
    while *pos < toks.len() {
        match &toks[*pos] {
            Tok::RBrace if stop_at_brace => return Ok(out),
            Tok::RBrace => return Err(MathError::UnbalancedBraces),
            Tok::LBrace => {
                *pos += 1;
                let inner = translate_seq(toks, pos, true)?;
                expect_rbrace(toks, pos)?;
                out.push('(');
                out.push_str(&inner);
                out.push(')');
            }
            Tok::Sup => {
                *pos += 1;
                out.push_str("^(");
                out.push_str(&read_unit(toks, pos)?);
                out.push(')');
            }
            Tok::Sub => {
                *pos += 1;
                out.push_str("_(");
                out.push_str(&read_unit(toks, pos)?);
                out.push(')');
            }
            Tok::Amp => {
                *pos += 1;
                out.push_str(" & ");
            }
            Tok::Ch(c) => {
                out.push(*c);
                out.push(' ');
                *pos += 1;
            }
            Tok::Ctrl(name) => {
                let name = name.clone();
                *pos += 1;
                out.push_str(&translate_command(&name, toks, pos)?);
                out.push(' ');
            }
        }
    }
    if stop_at_brace {
        Err(MathError::UnexpectedEnd)
    } else {
        Ok(out)
    }
}

/// Consumes an expected `}`.
fn expect_rbrace(toks: &[Tok], pos: &mut usize) -> Result<(), MathError> {
    match toks.get(*pos) {
        Some(Tok::RBrace) => {
            *pos += 1;
            Ok(())
        }
        _ => Err(MathError::UnbalancedBraces),
    }
}

/// Reads a single operand (a `{...}` group's contents, or one token).
fn read_unit(toks: &[Tok], pos: &mut usize) -> Result<String, MathError> {
    match toks.get(*pos) {
        Some(Tok::LBrace) => {
            *pos += 1;
            let inner = translate_seq(toks, pos, true)?;
            expect_rbrace(toks, pos)?;
            Ok(inner.trim().to_string())
        }
        Some(Tok::Ch(c)) => {
            let c = *c;
            *pos += 1;
            Ok(c.to_string())
        }
        Some(Tok::Ctrl(name)) => {
            let name = name.clone();
            *pos += 1;
            Ok(translate_command(&name, toks, pos)?.trim().to_string())
        }
        _ => Err(MathError::UnexpectedEnd),
    }
}

/// Reads the literal text of a `{...}` group (for `\text`).
fn read_literal_group(toks: &[Tok], pos: &mut usize) -> Result<String, MathError> {
    if toks.get(*pos) != Some(&Tok::LBrace) {
        return Err(MathError::MissingArgument("\\text"));
    }
    *pos += 1;
    let mut out = String::new();
    while let Some(t) = toks.get(*pos) {
        match t {
            Tok::RBrace => {
                *pos += 1;
                return Ok(out);
            }
            Tok::Ch(c) => out.push(*c),
            Tok::LBrace => out.push('{'),
            Tok::Sup => out.push('^'),
            Tok::Sub => out.push('_'),
            Tok::Amp => out.push('&'),
            Tok::Ctrl(c) => {
                out.push('\\');
                out.push_str(c);
            }
        }
        *pos += 1;
    }
    Err(MathError::UnbalancedBraces)
}

/// Translates one `\command` (plus any arguments it consumes).
fn translate_command(name: &str, toks: &[Tok], pos: &mut usize) -> Result<String, MathError> {
    // Structural commands.
    match name {
        "frac" | "tfrac" | "dfrac" => {
            let a = read_unit(toks, pos)?;
            let b = read_unit(toks, pos)?;
            return Ok(format!("frac({a}, {b})"));
        }
        "sqrt" => {
            // Optional [n] index.
            if toks.get(*pos) == Some(&Tok::Ch('[')) {
                *pos += 1;
                let mut idx = String::new();
                while let Some(t) = toks.get(*pos) {
                    if t == &Tok::Ch(']') {
                        *pos += 1;
                        break;
                    }
                    if let Tok::Ch(c) = t {
                        idx.push(*c);
                    }
                    *pos += 1;
                }
                let x = read_unit(toks, pos)?;
                return Ok(format!("root({idx}, {x})"));
            }
            let x = read_unit(toks, pos)?;
            return Ok(format!("sqrt({x})"));
        }
        "text" | "mathrm" | "operatorname" => {
            let s = read_literal_group(toks, pos)?;
            return Ok(format!("\"{s}\""));
        }
        "mathbb" => {
            let s = read_unit(toks, pos)?;
            return Ok(match s.as_str() {
                "R" => "RR".into(),
                "N" => "NN".into(),
                "Z" => "ZZ".into(),
                "Q" => "QQ".into(),
                "C" => "CC".into(),
                other => format!("upright({other})"),
            });
        }
        "vec" => return Ok(format!("arrow({})", read_unit(toks, pos)?)),
        "hat" | "widehat" => return Ok(format!("hat({})", read_unit(toks, pos)?)),
        "bar" | "overline" => return Ok(format!("overline({})", read_unit(toks, pos)?)),
        "dot" => return Ok(format!("dot({})", read_unit(toks, pos)?)),
        "ddot" => return Ok(format!("dot.double({})", read_unit(toks, pos)?)),
        "tilde" | "widetilde" => return Ok(format!("tilde({})", read_unit(toks, pos)?)),
        "left" | "right" => {
            // Emit the following delimiter (typst auto-sizes delimiters); `.`
            // means "no delimiter".
            return read_delimiter(toks, pos);
        }
        "begin" => {
            let env = read_literal_group(toks, pos)?;
            return translate_matrix(toks, pos, &env);
        }
        "end" => {
            // Consumed by translate_matrix; a stray \end is an error.
            let _ = read_literal_group(toks, pos);
            return Err(MathError::UnclosedEnvironment(name.to_string()));
        }
        _ => {}
    }

    // Symbol / operator table.
    if let Some(sym) = symbol(name) {
        return Ok(sym.to_string());
    }
    Err(MathError::UnknownCommand(name.to_string()))
}

/// Reads and maps a `\left` / `\right` delimiter token.
fn read_delimiter(toks: &[Tok], pos: &mut usize) -> Result<String, MathError> {
    match toks.get(*pos) {
        Some(Tok::Ch('.')) => {
            *pos += 1;
            Ok(String::new())
        }
        Some(Tok::Ch(c)) => {
            let c = *c;
            *pos += 1;
            Ok(c.to_string())
        }
        Some(Tok::Ctrl(name)) => {
            let name = name.clone();
            *pos += 1;
            translate_command(&name, toks, pos)
        }
        _ => Err(MathError::MissingArgument("delimiter")),
    }
}

/// Translates a matrix-like environment body up to and including its `\end`.
fn translate_matrix(toks: &[Tok], pos: &mut usize, env: &str) -> Result<String, MathError> {
    // Collect body tokens until the matching \end{env} (no nested matrices in
    // the supported corpus).
    let mut body: Vec<Tok> = Vec::new();
    loop {
        match toks.get(*pos) {
            None => return Err(MathError::UnclosedEnvironment(env.to_string())),
            Some(Tok::Ctrl(c)) if c == "end" => {
                *pos += 1;
                let _closing = read_literal_group(toks, pos)?;
                break;
            }
            Some(t) => {
                body.push(t.clone());
                *pos += 1;
            }
        }
    }

    // Split into rows (\\) then cells (&), translating each cell.
    let delim = match env {
        "pmatrix" => "\"(\"",
        "bmatrix" => "\"[\"",
        "Bmatrix" => "\"{\"",
        "vmatrix" => "\"|\"",
        "Vmatrix" => "\"||\"",
        _ => "#none",
    };
    let mut rows: Vec<Vec<Tok>> = vec![Vec::new()];
    for t in body {
        match t {
            Tok::Ctrl(ref c) if c == "\\" => rows.push(Vec::new()),
            other => rows.last_mut().unwrap().push(other),
        }
    }
    // Drop a trailing empty row (from a final `\\`).
    if rows.last().map(|r| r.iter().all(is_blank)).unwrap_or(false) {
        rows.pop();
    }

    let mut translated_rows = Vec::new();
    for row in rows {
        let mut cells = vec![Vec::new()];
        for t in row {
            match t {
                Tok::Amp => cells.push(Vec::new()),
                other => cells.last_mut().unwrap().push(other),
            }
        }
        let mut translated_cells = Vec::new();
        for cell in cells {
            let mut p = 0;
            translated_cells.push(translate_seq(&cell, &mut p, false)?.trim().to_string());
        }
        translated_rows.push(translated_cells.join(", "));
    }
    Ok(format!(
        "mat(delim: {delim}, {})",
        translated_rows.join("; ")
    ))
}

/// Whether a token is whitespace-only.
fn is_blank(t: &Tok) -> bool {
    matches!(t, Tok::Ch(c) if c.is_whitespace())
}

/// Maps a LaTeX symbol/operator command to its typst equivalent.
fn symbol(name: &str) -> Option<&'static str> {
    Some(match name {
        // Lowercase greek.
        "alpha" => "alpha",
        "beta" => "beta",
        "gamma" => "gamma",
        "delta" => "delta",
        "epsilon" => "epsilon",
        "varepsilon" => "epsilon.alt",
        "zeta" => "zeta",
        "eta" => "eta",
        "theta" => "theta",
        "vartheta" => "theta.alt",
        "iota" => "iota",
        "kappa" => "kappa",
        "lambda" => "lambda",
        "mu" => "mu",
        "nu" => "nu",
        "xi" => "xi",
        "pi" => "pi",
        "varpi" => "pi.alt",
        "rho" => "rho",
        "varrho" => "rho.alt",
        "sigma" => "sigma",
        "varsigma" => "sigma.alt",
        "tau" => "tau",
        "upsilon" => "upsilon",
        "phi" => "phi",
        "varphi" => "phi.alt",
        "chi" => "chi",
        "psi" => "psi",
        "omega" => "omega",
        // Uppercase greek.
        "Gamma" => "Gamma",
        "Delta" => "Delta",
        "Theta" => "Theta",
        "Lambda" => "Lambda",
        "Xi" => "Xi",
        "Pi" => "Pi",
        "Sigma" => "Sigma",
        "Upsilon" => "Upsilon",
        "Phi" => "Phi",
        "Psi" => "Psi",
        "Omega" => "Omega",
        // Binary operators.
        "cdot" => "dot.c",
        "times" => "times",
        "div" => "div",
        "pm" => "plus.minus",
        "mp" => "minus.plus",
        "ast" => "*",
        "star" => "star.op",
        "circ" => "compose",
        "bullet" => "bullet",
        "oplus" => "plus.circle",
        "otimes" => "times.circle",
        "cup" => "union",
        "cap" => "sect",
        "setminus" => "without",
        // Relations.
        "leq" | "le" => "<=",
        "geq" | "ge" => ">=",
        "neq" | "ne" => "eq.not",
        "approx" => "approx",
        "equiv" => "equiv",
        "cong" => "tilde.equiv",
        "sim" => "tilde.op",
        "propto" => "prop",
        "ll" => "lt.double",
        "gg" => "gt.double",
        "subset" => "subset",
        "subseteq" => "subset.eq",
        "supset" => "supset",
        "supseteq" => "supset.eq",
        "in" => "in",
        "notin" => "in.not",
        "ni" => "in.rev",
        "perp" => "perp",
        "parallel" => "parallel",
        // Arrows.
        "to" | "rightarrow" => "arrow.r",
        "leftarrow" => "arrow.l",
        "leftrightarrow" => "arrow.l.r",
        "Rightarrow" => "arrow.r.double",
        "Leftarrow" => "arrow.l.double",
        "Leftrightarrow" => "arrow.l.r.double",
        "mapsto" => "arrow.r.bar",
        // Big operators.
        "sum" => "sum",
        "prod" => "product",
        "coprod" => "product.co",
        "int" => "integral",
        "iint" => "integral.double",
        "oint" => "integral.cont",
        "bigcup" => "union.big",
        "bigcap" => "sect.big",
        "lim" => "lim",
        // Named functions (upright operators).
        "sin" => "sin",
        "cos" => "cos",
        "tan" => "tan",
        "cot" => "cot",
        "sec" => "sec",
        "csc" => "csc",
        "sinh" => "sinh",
        "cosh" => "cosh",
        "tanh" => "tanh",
        "log" => "log",
        "ln" => "ln",
        "exp" => "exp",
        "max" => "max",
        "min" => "min",
        "gcd" => "gcd",
        "det" => "det",
        "dim" => "dim",
        "ker" => "ker",
        "deg" => "deg",
        "arg" => "arg",
        // Misc symbols.
        "infty" => "infinity",
        "partial" => "diff",
        "nabla" => "nabla",
        "forall" => "forall",
        "exists" => "exists",
        "emptyset" => "nothing",
        "angle" => "angle",
        "hbar" => "planck.reduce",
        "ell" => "ell",
        "Re" => "Re",
        "Im" => "Im",
        "cdots" => "dots.c",
        "ldots" | "dots" => "dots.h",
        "vdots" => "dots.v",
        "ddots" => "dots.down",
        "langle" => "angle.l",
        "rangle" => "angle.r",
        "prime" => "prime",
        "circ2" => "degree",
        // Escapes.
        "{" => "{",
        "}" => "}",
        "|" => "||",
        "," => " ",
        ";" => " ",
        " " => " ",
        "%" => "%",
        "&" => "&",
        "#" => "\\#",
        "$" => "\\$",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tr(s: &str) -> String {
        translate(s).unwrap()
    }

    #[test]
    fn fraction() {
        assert_eq!(tr(r"\frac{a}{b}"), "frac(a, b)");
    }

    #[test]
    fn sqrt_and_root() {
        assert_eq!(tr(r"\sqrt{x}"), "sqrt(x)");
        assert_eq!(tr(r"\sqrt[3]{x}"), "root(3, x)");
    }

    #[test]
    fn super_and_subscript() {
        assert_eq!(tr("x^2"), "x ^(2)");
        assert_eq!(tr("x^{2n}"), "x ^(2 n)");
        assert_eq!(tr("a_i"), "a _(i)");
    }

    #[test]
    fn greek_maps_directly() {
        assert_eq!(tr(r"\pi"), "pi");
        assert_eq!(tr(r"\Omega"), "Omega");
        assert_eq!(tr(r"\varepsilon"), "epsilon.alt");
    }

    #[test]
    fn relations_and_operators() {
        assert_eq!(tr(r"a \leq b"), "a <= b");
        assert_eq!(tr(r"a \neq b"), "a eq.not b");
        assert_eq!(tr(r"a \cdot b"), "a dot.c b");
        assert_eq!(tr(r"x \to y"), "x arrow.r y");
    }

    #[test]
    fn big_operators_with_limits() {
        assert_eq!(tr(r"\sum_{i=0}^{n}"), "sum _(i = 0)^(n)");
        assert_eq!(tr(r"\int_0^1"), "integral _(0)^(1)");
    }

    #[test]
    fn mathbb_and_functions() {
        assert_eq!(tr(r"\mathbb{R}"), "RR");
        assert_eq!(tr(r"\sin x"), "sin x");
    }

    #[test]
    fn text_is_literal() {
        assert_eq!(tr(r"\text{if } x"), "\"if \" x");
    }

    #[test]
    fn matrix() {
        assert_eq!(
            tr(r"\begin{pmatrix} a & b \\ c & d \end{pmatrix}"),
            "mat(delim: \"(\", a, b; c, d)"
        );
    }

    #[test]
    fn accents() {
        assert_eq!(tr(r"\vec{v}"), "arrow(v)");
        assert_eq!(tr(r"\hat{x}"), "hat(x)");
    }

    #[test]
    fn unknown_command_errors() {
        assert_eq!(
            translate(r"\foobar"),
            Err(MathError::UnknownCommand("foobar".to_string()))
        );
    }

    #[test]
    fn unbalanced_braces_error() {
        assert!(translate("{a").is_err());
        assert!(translate("a}").is_err());
    }
}
