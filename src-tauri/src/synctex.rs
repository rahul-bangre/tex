//! A small SyncTeX reader: enough for forward (source → PDF) and inverse
//! (PDF → source) search.

use flate2::read::GzDecoder;
use serde::Serialize;
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};

/// Scaled points per PDF big point.
const SP_PER_BP: f64 = 65781.76;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    /// Horizontal box (`(` or `h`): has width/height/depth.
    HBox,
    /// Vertical box (`[` or `v`).
    VBox,
    /// A point record: x, g, k, $.
    Point,
}

#[derive(Clone, Copy, Debug)]
struct Node {
    kind: Kind,
    tag: u32,
    line: u32,
    h: i64,
    v: i64,
    w: i64,
    ht: i64,
    dp: i64,
}

pub struct Synctex {
    inputs: HashMap<u32, PathBuf>,
    pages: Vec<Vec<Node>>,
    scale: f64,
}

#[derive(Serialize, Debug)]
pub struct Rect {
    pub page: u32,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Serialize, Debug)]
pub struct SourceLoc {
    pub file: String,
    pub line: u32,
}

fn parse_int(s: &str) -> i64 {
    s.trim().parse().unwrap_or(0)
}

impl Synctex {
    pub fn load(path: &Path, base: &Path) -> Option<Synctex> {
        let bytes = std::fs::read(path).ok()?;
        let mut text = String::new();
        if path.extension().map(|e| e == "gz").unwrap_or(false) {
            GzDecoder::new(&bytes[..]).read_to_string(&mut text).ok()?;
        } else {
            text = String::from_utf8_lossy(&bytes).into_owned();
        }
        Some(Self::parse(&text, base))
    }

    pub fn parse(text: &str, base: &Path) -> Synctex {
        let mut inputs = HashMap::new();
        let mut pages: Vec<Vec<Node>> = Vec::new();
        let mut current: Option<usize> = None;
        let mut unit = 1.0;
        let mut mag = 1000.0;

        for line in text.lines() {
            if line.is_empty() {
                continue;
            }
            if let Some(rest) = line.strip_prefix("Input:") {
                if let Some((tag, name)) = rest.split_once(':') {
                    if !name.is_empty() {
                        let p = PathBuf::from(name);
                        let p = if p.is_absolute() { p } else { base.join(p) };
                        let p = std::fs::canonicalize(&p).unwrap_or(p);
                        inputs.insert(parse_int(tag) as u32, p);
                    }
                }
                continue;
            }
            if let Some(rest) = line.strip_prefix("Unit:") {
                unit = rest.trim().parse().unwrap_or(1.0);
                continue;
            }
            if let Some(rest) = line.strip_prefix("Magnification:") {
                mag = rest.trim().parse().unwrap_or(1000.0);
                continue;
            }
            let first = line.as_bytes()[0];
            match first {
                b'{' => {
                    let n = parse_int(&line[1..]).max(1) as usize;
                    while pages.len() < n {
                        pages.push(Vec::new());
                    }
                    current = Some(n - 1);
                }
                b'}' => current = None,
                b'(' | b'h' | b'[' | b'v' | b'x' | b'g' | b'k' | b'$' => {
                    let Some(pi) = current else { continue };
                    let body = &line[1..];
                    let Some((link, rest)) = body.split_once(':') else { continue };
                    let mut lk = link.split(',');
                    let tag = parse_int(lk.next().unwrap_or("0")) as u32;
                    let ln = parse_int(lk.next().unwrap_or("0")) as u32;
                    let mut parts = rest.split(':');
                    let pos = parts.next().unwrap_or("");
                    let (h, v) = match pos.split_once(',') {
                        Some((a, b)) => (parse_int(a), parse_int(b)),
                        None => continue,
                    };
                    let dims = parts.next().unwrap_or("");
                    let mut d = dims.split(',').map(parse_int);
                    let (w, ht, dp) = (d.next().unwrap_or(0), d.next().unwrap_or(0), d.next().unwrap_or(0));
                    let kind = match first {
                        b'(' | b'h' => Kind::HBox,
                        b'[' | b'v' => Kind::VBox,
                        _ => Kind::Point,
                    };
                    pages[pi].push(Node { kind, tag, line: ln, h, v, w, ht, dp });
                }
                _ => {}
            }
        }
        Synctex { inputs, pages, scale: unit * (mag / 1000.0) / SP_PER_BP }
    }

    fn tags_for(&self, file: &Path) -> Vec<u32> {
        let canon = std::fs::canonicalize(file).unwrap_or_else(|_| file.to_path_buf());
        self.inputs.iter().filter(|(_, p)| **p == canon).map(|(t, _)| *t).collect()
    }

    /// Where in the PDF does `file:line` land?
    pub fn forward(&self, file: &Path, line: u32) -> Option<Rect> {
        let tags = self.tags_for(file);
        if tags.is_empty() {
            return None;
        }
        // Choose the closest line that actually produced output, preferring lines at or after.
        let mut best: Option<u32> = None;
        for page in &self.pages {
            for n in page {
                if n.kind == Kind::VBox || !tags.contains(&n.tag) || n.line == 0 {
                    continue;
                }
                let better = match best {
                    None => true,
                    Some(b) => dist(n.line, line) < dist(b, line),
                };
                if better {
                    best = Some(n.line);
                }
            }
        }
        let target = best?;

        for (pi, page) in self.pages.iter().enumerate() {
            let hits: Vec<&Node> = page
                .iter()
                .filter(|n| n.kind != Kind::VBox && tags.contains(&n.tag) && n.line == target)
                .collect();
            if hits.is_empty() {
                continue;
            }
            let points: Vec<&&Node> = hits.iter().filter(|n| n.kind == Kind::Point).collect();
            let (x0, y0, x1, y1) = if !points.is_empty() {
                // Point records sit on the baseline: build a line-height band.
                let x0 = points.iter().map(|n| n.h).min().unwrap() as f64;
                let x1 = points.iter().map(|n| n.h + n.w.max(0)).max().unwrap() as f64;
                let y0 = points.iter().map(|n| n.v).min().unwrap() as f64 - 9.0 / self.scale;
                let y1 = points.iter().map(|n| n.v).max().unwrap() as f64 + 3.0 / self.scale;
                (x0, y0, x1, y1)
            } else {
                let x0 = hits.iter().map(|n| n.h).min().unwrap() as f64;
                let x1 = hits.iter().map(|n| n.h + n.w).max().unwrap() as f64;
                let y0 = hits.iter().map(|n| n.v - n.ht).min().unwrap() as f64;
                let y1 = hits.iter().map(|n| n.v + n.dp).max().unwrap() as f64;
                (x0, y0, x1, y1)
            };
            let s = self.scale;
            let w = ((x1 - x0) * s).max(24.0);
            return Some(Rect { page: pi as u32 + 1, x: x0 * s, y: y0 * s, w, h: ((y1 - y0) * s).max(10.0) });
        }
        None
    }

    /// Which source line produced the content at (`x`, `y`) bp on `page` (1-based)?
    pub fn inverse(&self, page: u32, x: f64, y: f64) -> Option<SourceLoc> {
        let nodes = self.pages.get(page.checked_sub(1)? as usize)?;
        let (px, py) = (x / self.scale, y / self.scale);
        let px = px as i64;
        let py = py as i64;

        // Smallest hbox containing the point.
        let container = nodes
            .iter()
            .filter(|n| n.kind == Kind::HBox && self.inputs.contains_key(&n.tag))
            .filter(|n| px >= n.h && px <= n.h + n.w && py >= n.v - n.ht && py <= n.v + n.dp)
            .min_by_key(|n| (n.w as i128) * ((n.ht + n.dp) as i128));

        let pick = if let Some(b) = container {
            nodes
                .iter()
                .filter(|n| n.kind == Kind::Point && self.inputs.contains_key(&n.tag))
                .filter(|n| n.v >= b.v - b.ht && n.v <= b.v + b.dp && n.h >= b.h && n.h <= b.h + b.w)
                .min_by_key(|n| (n.h - px).abs())
                .copied()
                .or(Some(*b))
        } else {
            nodes
                .iter()
                .filter(|n| n.kind != Kind::VBox && self.inputs.contains_key(&n.tag) && n.line > 0)
                .min_by_key(|n| {
                    let dx = (n.h - px) as f64;
                    let dy = (n.v - py) as f64 * 4.0;
                    (dx * dx + dy * dy) as i128
                })
                .copied()
        }?;

        let file = self.inputs.get(&pick.tag)?;
        Some(SourceLoc { file: file.to_string_lossy().to_string(), line: pick.line.max(1) })
    }
}

fn dist(a: u32, target: u32) -> (u32, u32) {
    // Prefer lines at/after the target on ties.
    if a >= target {
        (a - target, 0)
    } else {
        (target - a, 1)
    }
}

/// Cache of parsed synctex files keyed by path and mtime.
#[derive(Default)]
pub struct Cache {
    entries: HashMap<PathBuf, (u64, std::sync::Arc<Synctex>)>,
}

impl Cache {
    pub fn get(&mut self, path: &Path, base: &Path) -> Option<std::sync::Arc<Synctex>> {
        let m = crate::library::mtime_ms(path);
        if let Some((mt, s)) = self.entries.get(path) {
            if *mt == m {
                return Some(s.clone());
            }
        }
        let s = std::sync::Arc::new(Synctex::load(path, base)?);
        self.entries.insert(path.to_path_buf(), (m, s.clone()));
        Some(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "SyncTeX Version:1
Input:1:/tmp/tex-sync/main.tex
Input:2:
Output:pdf
Magnification:1000
Unit:1
X Offset:0
Y Offset:0
Content:
!347
{1
[1,10:4736287,49328948:30785865,44592661,0
(1,7:4736287,6827737:30785865,454820,135003
g1,6:6429082,6827737
g1,6:8233288,6827737
g1,6:11369186,6827737
k1,7:35522152,6827737:22767535
)
(1,7:18855024,7614169:2548391,566233,0
x1,7:19338807,7614169
)
]
}1
";

    #[test]
    fn forward_and_inverse() {
        let s = Synctex::parse(SAMPLE, Path::new("/"));
        let file = Path::new("/tmp/tex-sync/main.tex");
        let r = s.forward(file, 6).expect("forward");
        assert_eq!(r.page, 1);
        // 1in margin => 72bp, first glue at 6429082sp ≈ 97.7bp.
        assert!((r.x - 97.7).abs() < 0.5, "{r:?}");
        assert!((r.y - (6827737.0 / SP_PER_BP - 9.0)).abs() < 0.5);

        let loc = s.inverse(1, 120.0, 100.0).expect("inverse");
        assert_eq!(loc.line, 6);
        // Display math line.
        let loc = s.inverse(1, 295.0, 112.0).expect("inverse math");
        assert_eq!(loc.line, 7);
    }
}
