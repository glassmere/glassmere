//! Converts a BDF bitmap font into the data `BitmapFont` reads: a `.bin` file
//! of glyph rows and a `.rs` file that wraps it in a static.
//!
//! ```text
//! cargo run -p glassmere-embedded --example bdf_convert -- <font.bdf> <out-dir> <name> [ranges]
//! ```
//!
//! `name` is the file stem, such as `fixed_6x13`; the static is that in upper
//! case. `ranges` lists the code points to keep in hexadecimal, such as
//! `20-7E,A0-17F,20AC`; without it `DEFAULT_RANGES` is used. Code points the
//! font does not have are left out, and a glyph that is not as wide as the
//! cell is skipped, because the renderer lays glyphs out on a fixed grid.
//!
//! Each glyph is `height` rows of `ceil(width / 8)` bytes, the leftmost pixel
//! in the top bit, placed in the cell so that the baseline sits `ascent` rows
//! from its top. Run `rustfmt` on the `.rs` output.
use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::{env, fs, process};

/// Latin-1 and Latin Extended-A, typographic punctuation, a few arrows and
/// symbols the controls use.
const DEFAULT_RANGES: &str = "20-7E,A0-17F,2010-2015,2018-201E,2020-2022,2026,2030,2039-203A,\
    20AC,2122,2190-2195,2212,2260,2264-2265,2500,2502,25A0-25A1,25B2,25B6,25B8,25BC,25BE,\
    25C0,25CB,25CF,2610-2612,2713-2717";

struct Glyph {
    width: i32,
    height: i32,
    x: i32,
    y: i32,
    advance: i32,
    rows: Vec<Vec<u8>>,
}

fn parse_ranges(spec: &str) -> Result<BTreeSet<u32>, String> {
    let mut set = BTreeSet::new();
    for part in spec.split(',').filter(|part| !part.trim().is_empty()) {
        let (first, last) = part.split_once('-').unwrap_or((part, part));
        let parse = |text: &str| {
            u32::from_str_radix(text.trim(), 16).map_err(|_| format!("bad code point {text:?}"))
        };
        let (first, last) = (parse(first)?, parse(last)?);
        set.extend(first..=last);
    }
    Ok(set)
}

fn number(token: Option<&str>) -> Result<i32, String> {
    token
        .ok_or("missing number")?
        .parse()
        .map_err(|_| "bad number".to_owned())
}

fn main() {
    let arguments: Vec<String> = env::args().skip(1).collect();
    if arguments.len() < 3 {
        eprintln!("usage: bdf_convert <font.bdf> <out-dir> <name> [ranges]");
        process::exit(2);
    }
    if let Err(message) = convert(
        &arguments[0],
        &arguments[1],
        &arguments[2],
        arguments.get(3).map_or(DEFAULT_RANGES, String::as_str),
    ) {
        eprintln!("bdf_convert: {message}");
        process::exit(1);
    }
}

fn convert(input: &str, out_dir: &str, name: &str, ranges: &str) -> Result<(), String> {
    let wanted = parse_ranges(ranges)?;
    let source = fs::read_to_string(input).map_err(|error| format!("{input}: {error}"))?;
    let mut cell = None;
    let mut ascent = None;
    let mut glyphs: Vec<(u32, Glyph)> = Vec::new();
    let mut lines = source.lines();
    while let Some(line) = lines.next() {
        let mut tokens = line.split_whitespace();
        match tokens.next() {
            Some("FONTBOUNDINGBOX") => {
                let width = number(tokens.next())?;
                let height = number(tokens.next())?;
                let x = number(tokens.next())?;
                let y = number(tokens.next())?;
                cell = Some((width, height, x, y));
            }
            Some("FONT_ASCENT") => ascent = Some(number(tokens.next())?),
            Some("ENCODING") => {
                let code = number(tokens.next())?;
                let mut glyph = Glyph {
                    width: 0,
                    height: 0,
                    x: 0,
                    y: 0,
                    advance: 0,
                    rows: Vec::new(),
                };
                let mut in_bitmap = false;
                for line in lines.by_ref() {
                    let mut tokens = line.split_whitespace();
                    let first = tokens.next();
                    match first {
                        Some("ENDCHAR") => break,
                        Some(hex) if in_bitmap => {
                            let bytes = (0..hex.len() / 2)
                                .map(|at| u8::from_str_radix(&hex[at * 2..at * 2 + 2], 16))
                                .collect::<Result<Vec<_>, _>>()
                                .map_err(|_| format!("bad bitmap row {hex:?}"))?;
                            glyph.rows.push(bytes);
                        }
                        Some("DWIDTH") => glyph.advance = number(tokens.next())?,
                        Some("BBX") => {
                            glyph.width = number(tokens.next())?;
                            glyph.height = number(tokens.next())?;
                            glyph.x = number(tokens.next())?;
                            glyph.y = number(tokens.next())?;
                        }
                        Some("BITMAP") => in_bitmap = true,
                        _ => {}
                    }
                }
                if let Ok(code) = u32::try_from(code)
                    && wanted.contains(&code)
                {
                    glyphs.push((code, glyph));
                }
            }
            _ => {}
        }
    }
    let (cell_width, cell_height, _, cell_y) = cell.ok_or("no FONTBOUNDINGBOX")?;
    let ascent = ascent.unwrap_or(cell_height + cell_y);
    let row_bytes = (cell_width as usize).div_ceil(8);
    glyphs.sort_by_key(|(code, _)| *code);
    glyphs.dedup_by_key(|(code, _)| *code);

    let mut data = Vec::new();
    let mut runs: Vec<(u32, u32, usize)> = Vec::new();
    let mut skipped = 0;
    for (code, glyph) in &glyphs {
        if glyph.advance != cell_width {
            skipped += 1;
            continue;
        }
        let mut cell = vec![0u8; row_bytes * cell_height as usize];
        let top = ascent - (glyph.y + glyph.height);
        for (row, bits) in glyph.rows.iter().enumerate() {
            let cell_row = top + row as i32;
            if !(0..cell_height).contains(&cell_row) {
                continue;
            }
            for column in 0..glyph.width {
                let set = bits
                    .get(column as usize / 8)
                    .is_some_and(|byte| byte & (0x80 >> (column % 8)) != 0);
                let cell_column = glyph.x + column;
                if set && (0..cell_width).contains(&cell_column) {
                    cell[cell_row as usize * row_bytes + cell_column as usize / 8] |=
                        0x80 >> (cell_column % 8);
                }
            }
        }
        let index = data.len() / (row_bytes * cell_height as usize);
        match runs.last_mut() {
            Some(run) if run.1 + 1 == *code => run.1 = *code,
            _ => runs.push((*code, *code, index)),
        }
        data.extend_from_slice(&cell);
    }

    let mut code = String::new();
    writeln!(
        code,
        "//! Generated by `examples/bdf_convert.rs` from `{}`. Do not edit.\n",
        PathBuf::from(input)
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
    )
    .unwrap();
    writeln!(code, "use crate::font::{{BitmapFont, GlyphRange}};\n").unwrap();
    writeln!(
        code,
        "pub static {}: BitmapFont = BitmapFont::new(\n    {cell_width},\n    {cell_height},\n    {ascent},\n    &[",
        name.to_uppercase()
    )
    .unwrap();
    for (first, last, index) in &runs {
        writeln!(
            code,
            "        GlyphRange::new({first:#x}, {last:#x}, {index}),"
        )
        .unwrap();
    }
    writeln!(code, "    ],\n    include_bytes!(\"{name}.bin\"),\n);").unwrap();

    let out = PathBuf::from(out_dir);
    fs::create_dir_all(&out).map_err(|error| error.to_string())?;
    fs::write(out.join(format!("{name}.bin")), &data).map_err(|error| error.to_string())?;
    fs::write(out.join(format!("{name}.rs")), code).map_err(|error| error.to_string())?;
    let count = data.len() / (row_bytes * cell_height as usize);
    println!(
        "{name}: {cell_width}x{cell_height}, ascent {ascent}, {count} glyphs in {} ranges, {} bytes{}",
        runs.len(),
        data.len(),
        if skipped > 0 {
            format!(", {skipped} skipped for a different advance")
        } else {
            String::new()
        }
    );
    Ok(())
}
