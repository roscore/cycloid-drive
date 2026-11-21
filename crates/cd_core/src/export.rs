//! Disc profile export (mm, disc frame): SVG and DXF (one closed LWPOLYLINE for the profile,
//! CIRCLE entities for the bore and the output holes).
use crate::Design;
use std::fmt::Write;

/// Output-hole centres of disc `disc` in its own frame (on the output pin circle).
pub fn hole_centres_of(d: &Design, disc: usize) -> Vec<[f64; 2]> {
    (0..d.p.zw as usize).map(|j| {
        let a = d.p.hole_angle(j, disc);
        [d.p.rw * a.cos(), d.p.rw * a.sin()]
    }).collect()
}

/// Output-hole centres of disc 0 (hole 0 on +x); exports describe this disc. Disc 1's holes are
/// turned by π/Zd relative to its lobes (`Params::hole_angle`): the same part turned by π when Zd is
/// odd and the output-pin count even.
pub fn hole_centres(d: &Design) -> Vec<[f64; 2]> {
    hole_centres_of(d, 0)
}

/// Standalone SVG drawing, 1 user unit = 1 mm, y up as in CAD (flipped with a transform).
pub fn svg(d: &Design) -> String {
    let r = d.r_max + 2.0;
    let mut s = String::new();
    let _ = write!(
        s,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w:.3}mm\" height=\"{w:.3}mm\" viewBox=\"{o:.3} {o:.3} {w:.3} {w:.3}\">\n<g transform=\"scale(1,-1)\" fill=\"none\" stroke=\"black\" stroke-width=\"0.1\">\n<path d=\"",
        w = 2.0 * r,
        o = -r
    );
    for (i, p) in d.prof.iter().enumerate() {
        let _ = write!(s, "{}{:.4} {:.4} ", if i == 0 { "M" } else { "L" }, p[0], p[1]);
    }
    s.push_str("Z\"/>\n");
    let _ = writeln!(s, "<circle cx=\"0\" cy=\"0\" r=\"{:.4}\"/>", d.p.bore / 2.0);
    for c in hole_centres(d) {
        let _ = writeln!(s, "<circle cx=\"{:.4}\" cy=\"{:.4}\" r=\"{:.4}\"/>", c[0], c[1], d.hole_d / 2.0);
    }
    s.push_str("</g>\n</svg>\n");
    s
}

/// Minimal DXF (AC1015 header with mm units, entities with handles and subclass markers).
/// Profile = closed LWPOLYLINE with `n` vertices.
pub fn dxf(d: &Design) -> String {
    let mut s = String::from("0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1015\n9\n$INSUNITS\n70\n4\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n");
    let _ = write!(s, "0\nLWPOLYLINE\n5\n100\n100\nAcDbEntity\n8\nPROFILE\n100\nAcDbPolyline\n90\n{}\n70\n1\n", d.prof.len());
    for p in &d.prof {
        let _ = write!(s, "10\n{:.5}\n20\n{:.5}\n", p[0], p[1]);
    }
    let mut handle = 0x100;
    let mut circle = |c: [f64; 2], r: f64, layer: &str| {
        handle += 1;
        let _ = write!(s, "0\nCIRCLE\n5\n{handle:X}\n100\nAcDbEntity\n8\n{layer}\n100\nAcDbCircle\n10\n{:.5}\n20\n{:.5}\n30\n0.0\n40\n{:.5}\n", c[0], c[1], r);
    };
    circle([0.0, 0.0], d.p.bore / 2.0, "BORE");
    for c in hole_centres(d) {
        circle(c, d.hole_d / 2.0, "HOLES");
    }
    s.push_str("0\nENDSEC\n0\nEOF\n");
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Params;

    #[test]
    fn dxf_header_and_vertex_count() {
        let d = Params { n: 240, ..Params::default() }.eval();
        let s = dxf(&d);
        assert!(s.starts_with("0\nSECTION\n2\nHEADER\n"));
        assert!(s.ends_with("0\nEOF\n"));
        // Count code-10 groups inside the LWPOLYLINE only (CIRCLE entities also use code 10).
        let lines: Vec<&str> = s.lines().collect();
        let start = lines.iter().position(|l| *l == "LWPOLYLINE").unwrap();
        let end = start + lines[start..].iter().position(|l| *l == "CIRCLE").unwrap();
        let verts = lines[start + 1..end - 1].chunks(2).filter(|g| g[0] == "10").count();
        assert_eq!(verts, 240);
        assert!(s.contains("\n90\n240\n"));
        assert_eq!(s.matches("CIRCLE").count(), 1 + d.p.zw as usize);
    }

    #[test]
    fn svg_has_profile_and_holes() {
        let d = Params::default().eval();
        let s = svg(&d);
        assert!(s.starts_with("<svg") && s.trim_end().ends_with("</svg>"));
        assert_eq!(s.matches("<circle").count(), 1 + d.p.zw as usize);
        assert_eq!(s.matches(" L").count(), d.prof.len() - 1);
    }
}
