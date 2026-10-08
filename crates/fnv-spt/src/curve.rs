//! The text curves of `.spt` files (docs/spec-notes/speedtree.md §2, §4).

/// `BezierSpline <min> <max> <variance> { n, n × (x y tx ty len) }`: a curve y(x) on
/// [0, 1] scaled to [min, max], with a random spread of ±variance.
#[derive(Debug, Clone, PartialEq)]
pub struct Curve {
    pub min: f32,
    pub max: f32,
    pub variance: f32,
    /// Control points: x, y, tangent x, tangent y, tangent length.
    pub points: Vec<[f32; 5]>,
}

impl Curve {
    pub fn parse(s: &str) -> Result<Curve, String> {
        let rest = s.trim().strip_prefix("BezierSpline").ok_or("not a BezierSpline")?;
        let (head, body) = rest.split_once('{').ok_or("no '{'")?;
        let body = body.split('}').next().unwrap_or_default();
        let num = |w: &str| w.parse::<f32>().map_err(|_| format!("bad number {w:?}"));
        let head: Vec<f32> = head.split_whitespace().map(num).collect::<Result<_, _>>()?;
        let [min, max, variance] = head[..] else { return Err("expected min, max, variance".into()) };
        let mut words = body.split_whitespace();
        let n: usize = words.next().ok_or("no point count")?.parse().map_err(|_| "bad point count")?;
        let nums: Vec<f32> = words.map(num).collect::<Result<_, _>>()?;
        if nums.len() != 5 * n || n == 0 {
            return Err(format!("{n} points but {} numbers", nums.len()));
        }
        let points = nums.as_chunks::<5>().0.to_vec();
        Ok(Curve { min, max, variance, points })
    }

    /// y at x (both 0-1): a cubic Bézier per span between neighbouring points, with
    /// handles at each point's tangent × length [hypothesis].
    pub fn y(&self, x: f32) -> f32 {
        let p = &self.points;
        let x = x.clamp(0.0, 1.0);
        if p.len() == 1 {
            return p[0][1];
        }
        let i = p.windows(2).position(|w| x <= w[1][0]).unwrap_or(p.len() - 2);
        let (a, b) = (p[i], p[i + 1]);
        let c0 = [a[0], a[1]];
        let c1 = [a[0] + a[2] * a[4], a[1] + a[3] * a[4]];
        let c2 = [b[0] - b[2] * b[4], b[1] - b[3] * b[4]];
        let c3 = [b[0], b[1]];
        let at = |t: f32, k: usize| {
            let u = 1.0 - t;
            u * u * u * c0[k] + 3.0 * u * u * t * c1[k] + 3.0 * u * t * t * c2[k] + t * t * t * c3[k]
        };
        // x(t) rises from a.x to b.x for the handles seen in the files; find t by bisection.
        let (mut lo, mut hi) = (0.0f32, 1.0f32);
        for _ in 0..24 {
            let mid = 0.5 * (lo + hi);
            if at(mid, 0) < x {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        at(0.5 * (lo + hi), 1)
    }

    /// The value at x without spread: min + (max − min) · y(x).
    pub fn at(&self, x: f32) -> f32 {
        self.min + (self.max - self.min) * self.y(x)
    }

    /// The value at x plus a spread of ±variance; `r` is uniform in [0, 1).
    pub fn sample(&self, x: f32, r: f32) -> f32 {
        self.at(x) + self.variance * (2.0 * r - 1.0)
    }
}
