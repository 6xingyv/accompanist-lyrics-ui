//! G2-continuous rounded rectangles for Skia clips.
//!
//! This is a Rust/Skia port of the geometry core from Kyant0/Capsule at commit
//! `830975a0515e94444c59cf5dccaf35909a1c0f50`:
//! <https://github.com/Kyant0/Capsule>
//!
//! Capsule is licensed under Apache-2.0. The Compose shape wrappers are not
//! carried over; this module retains the continuity profile, constrained-corner
//! resolution and line -> cubic -> circular arc -> cubic path construction used
//! by the renderer's album-art clip.

use skia_safe::{Path, PathBuilder, Point as SkPoint, Rect};
use std::f64::consts::PI;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct G2Profile {
    pub extended_fraction: f64,
    pub arc_fraction: f64,
    pub bezier_curvature_scale: f64,
    pub arc_curvature_scale: f64,
}

impl G2Profile {
    /// Capsule's default rounded-rectangle curvature profile.
    pub const ROUNDED_RECTANGLE: Self = Self {
        extended_fraction: 0.528_665_1,
        arc_fraction: 5.0 / 9.0,
        bezier_curvature_scale: 1.073_205_1,
        arc_curvature_scale: 1.073_205_1,
    };

    /// Profile used while a corner is constrained toward a capsule.
    pub const CAPSULE: Self = Self {
        extended_fraction: 0.528_665_1 * 0.75,
        arc_fraction: 0.0,
        bezier_curvature_scale: 1.0,
        arc_curvature_scale: 1.0,
    };
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct CornerRadii {
    pub top_left: f32,
    pub top_right: f32,
    pub bottom_right: f32,
    pub bottom_left: f32,
}

impl CornerRadii {
    pub const fn uniform(radius: f32) -> Self {
        Self {
            top_left: radius,
            top_right: radius,
            bottom_right: radius,
            bottom_left: radius,
        }
    }

    fn normalized(self, width: f64, height: f64) -> [f64; 4] {
        let mut radii = [
            finite_non_negative(self.top_left),
            finite_non_negative(self.top_right),
            finite_non_negative(self.bottom_right),
            finite_non_negative(self.bottom_left),
        ];
        let ratios = [
            fit_ratio(width, radii[0] + radii[1]),
            fit_ratio(width, radii[3] + radii[2]),
            fit_ratio(height, radii[0] + radii[3]),
            fit_ratio(height, radii[1] + radii[2]),
        ];
        let scale = ratios.into_iter().fold(1.0_f64, f64::min);
        for radius in &mut radii {
            *radius *= scale;
        }
        radii
    }
}

/// Build Capsule's default G2-continuous rounded rectangle.
pub(crate) fn continuous_rounded_rect(rect: Rect, radius: f32) -> Path {
    continuous_rounded_rect_with_profile(
        rect,
        CornerRadii::uniform(radius),
        G2Profile::ROUNDED_RECTANGLE,
        G2Profile::CAPSULE,
    )
}

pub(crate) fn continuous_rounded_rect_with_profile(
    rect: Rect,
    radii: CornerRadii,
    profile: G2Profile,
    capsule_profile: G2Profile,
) -> Path {
    let width = rect.width().max(0.0) as f64;
    let height = rect.height().max(0.0) as f64;
    if width <= 0.0 || height <= 0.0 {
        return Path::new();
    }

    let [tl, tr, br, bl] = radii.normalized(width, height);
    if tl == 0.0 && tr == 0.0 && br == 0.0 && bl == 0.0 {
        let mut builder = PathBuilder::new();
        builder.add_rect(rect, None, None);
        return builder.detach();
    }

    let center_x = width * 0.5;
    let center_y = height * 0.5;
    let tl = Corner::resolve(tl, center_y, center_x, profile, capsule_profile);
    let tr = Corner::resolve(tr, center_y, center_x, profile, capsule_profile);
    let br = Corner::resolve(br, center_y, center_x, profile, capsule_profile);
    let bl = Corner::resolve(bl, center_y, center_x, profile, capsule_profile);
    let mut path = Builder::new(rect.left as f64, rect.top as f64);

    let mut x = 0.0;
    let mut y = tl.radius;
    path.move_to(x, y - tl.offset_v);

    // Top-left: vertical cubic, arc, horizontal cubic.
    if tl.radius > 0.0 {
        let b = tl.bezier_v;
        path.cubic_to(
            x + b.p1.y * tl.radius,
            y - b.p1.x * tl.radius,
            x + b.p2.y * tl.radius,
            y - b.p2.x * tl.radius,
            x + b.p3.y * tl.radius,
            y - b.p3.x * tl.radius,
        );
        path.arc_to_scaled(
            Point::new(tl.radius, tl.radius),
            tl.radius,
            1.0 / tl.arc_curvature_scale,
            PI + PI * 0.25 * (1.0 - tl.arc_fraction),
            PI * 0.5 * tl.arc_fraction,
        );
        x = tl.radius;
        y = 0.0;
        let b = tl.bezier_h;
        path.cubic_to(
            x - b.p2.x * tl.radius,
            y + b.p2.y * tl.radius,
            x - b.p1.x * tl.radius,
            y + b.p1.y * tl.radius,
            x - (b.p0.x * tl.radius).max(tl.offset_h),
            y + b.p0.y * tl.radius,
        );
    }

    // Top edge and top-right.
    x = width - tr.radius;
    y = 0.0;
    path.line_to(x + tr.offset_h, y);
    if tr.radius > 0.0 {
        let b = tr.bezier_h;
        path.cubic_to(
            x + b.p1.x * tr.radius,
            y + b.p1.y * tr.radius,
            x + b.p2.x * tr.radius,
            y + b.p2.y * tr.radius,
            x + b.p3.x * tr.radius,
            y + b.p3.y * tr.radius,
        );
        path.arc_to_scaled(
            Point::new(width - tr.radius, tr.radius),
            tr.radius,
            1.0 / tr.arc_curvature_scale,
            -PI * 0.5 + PI * 0.25 * (1.0 - tr.arc_fraction),
            PI * 0.5 * tr.arc_fraction,
        );
        x = width;
        y = tr.radius;
        let b = tr.bezier_v;
        path.cubic_to(
            x - b.p2.y * tr.radius,
            y - b.p2.x * tr.radius,
            x - b.p1.y * tr.radius,
            y - b.p1.x * tr.radius,
            x - b.p0.y * tr.radius,
            y - (b.p0.x * tr.radius).max(tr.offset_v),
        );
    }

    // Right edge and bottom-right.
    x = width;
    y = height - br.radius;
    path.line_to(x, y + br.offset_v);
    if br.radius > 0.0 {
        let b = br.bezier_v;
        path.cubic_to(
            x - b.p1.y * br.radius,
            y + b.p1.x * br.radius,
            x - b.p2.y * br.radius,
            y + b.p2.x * br.radius,
            x - b.p3.y * br.radius,
            y + b.p3.x * br.radius,
        );
        path.arc_to_scaled(
            Point::new(width - br.radius, height - br.radius),
            br.radius,
            1.0 / br.arc_curvature_scale,
            PI * 0.25 * (1.0 - br.arc_fraction),
            PI * 0.5 * br.arc_fraction,
        );
        x = width - br.radius;
        y = height;
        let b = br.bezier_h;
        path.cubic_to(
            x + b.p2.x * br.radius,
            y - b.p2.y * br.radius,
            x + b.p1.x * br.radius,
            y - b.p1.y * br.radius,
            x + (b.p0.x * br.radius).max(br.offset_h),
            y - b.p0.y * br.radius,
        );
    }

    // Bottom edge and bottom-left.
    x = bl.radius;
    y = height;
    path.line_to(x - bl.offset_h, y);
    if bl.radius > 0.0 {
        let b = bl.bezier_h;
        path.cubic_to(
            x - b.p1.x * bl.radius,
            y - b.p1.y * bl.radius,
            x - b.p2.x * bl.radius,
            y - b.p2.y * bl.radius,
            x - b.p3.x * bl.radius,
            y - b.p3.y * bl.radius,
        );
        path.arc_to_scaled(
            Point::new(bl.radius, height - bl.radius),
            bl.radius,
            1.0 / bl.arc_curvature_scale,
            PI * 0.5 + PI * 0.25 * (1.0 - bl.arc_fraction),
            PI * 0.5 * bl.arc_fraction,
        );
        x = 0.0;
        y = height - bl.radius;
        let b = bl.bezier_v;
        path.cubic_to(
            x + b.p2.y * bl.radius,
            y + b.p2.x * bl.radius,
            x + b.p1.y * bl.radius,
            y + b.p1.x * bl.radius,
            x + b.p0.y * bl.radius,
            y + (b.p0.x * bl.radius).max(bl.offset_v),
        );
    }

    path.close();
    path.finish()
}

#[derive(Clone, Copy, Debug)]
struct Corner {
    radius: f64,
    offset_v: f64,
    offset_h: f64,
    arc_fraction: f64,
    arc_curvature_scale: f64,
    bezier_v: CubicBezier,
    bezier_h: CubicBezier,
}

impl Corner {
    fn resolve(
        radius: f64,
        half_v: f64,
        half_h: f64,
        profile: G2Profile,
        capsule: G2Profile,
    ) -> Self {
        if radius <= 0.0 {
            return Self {
                radius: 0.0,
                offset_v: 0.0,
                offset_h: 0.0,
                arc_fraction: profile.arc_fraction,
                arc_curvature_scale: profile.arc_curvature_scale,
                bezier_v: CubicBezier::default(),
                bezier_h: CubicBezier::default(),
            };
        }
        let ratio_v = constrained_ratio(half_v, radius, profile.extended_fraction);
        let ratio_h = constrained_ratio(half_h, radius, profile.extended_fraction);
        let ratio = ratio_v.min(ratio_h);
        let extended = lerp(capsule.extended_fraction, profile.extended_fraction, ratio);
        let extended_v = extended * ratio_v;
        let extended_h = extended * ratio_h;
        let arc_fraction = lerp(capsule.arc_fraction, profile.arc_fraction, ratio);
        let arc_curvature_scale = 1.0 + (profile.arc_curvature_scale - 1.0) * ratio;
        let bezier_v = G2Profile {
            extended_fraction: extended_v,
            arc_fraction,
            bezier_curvature_scale: lerp(
                capsule.bezier_curvature_scale,
                profile.bezier_curvature_scale,
                ratio_v,
            ),
            arc_curvature_scale,
        }
        .bezier();
        let bezier_h = G2Profile {
            extended_fraction: extended_h,
            arc_fraction,
            bezier_curvature_scale: lerp(
                capsule.bezier_curvature_scale,
                profile.bezier_curvature_scale,
                ratio_h,
            ),
            arc_curvature_scale,
        }
        .bezier();
        Self {
            radius,
            offset_v: -radius * extended_v,
            offset_h: -radius * extended_h,
            arc_fraction,
            arc_curvature_scale,
            bezier_v,
            bezier_h,
        }
    }
}

impl G2Profile {
    fn bezier(self) -> CubicBezier {
        let arc_radians = PI * 0.5 * self.arc_fraction;
        let bezier_radians = (PI * 0.5 - arc_radians) * 0.5;
        let sin = bezier_radians.sin();
        let cos = bezier_radians.cos();
        if self.bezier_curvature_scale == 1.0 && self.arc_curvature_scale == 1.0 {
            let half_tan = sin / (1.0 + cos);
            return CubicBezier {
                p0: Point::new(-self.extended_fraction, 0.0),
                p1: Point::new((1.0 - 1.5 / (1.0 + cos)) * half_tan, 0.0),
                p2: Point::new(half_tan, 0.0),
                p3: Point::new(sin, 1.0 - cos),
            };
        }

        let radius_scale = 1.0 / self.arc_curvature_scale;
        let arc_center = Point::new(0.0, 1.0)
            + Point::new(1.0 / 2.0_f64.sqrt(), -1.0 / 2.0_f64.sqrt()) * (1.0 - radius_scale);
        let arc_start = arc_center + Point::new(sin, -cos) * radius_scale;
        g2_bezier_with_zero_start_curvature(
            Point::new(-self.extended_fraction, 0.0),
            arc_start,
            Point::new(1.0, 0.0),
            Point::new(cos, sin),
            self.bezier_curvature_scale,
        )
    }
}

fn g2_bezier_with_zero_start_curvature(
    start: Point,
    end: Point,
    start_tangent: Point,
    end_tangent: Point,
    end_curvature: f64,
) -> CubicBezier {
    let a2 = 1.5 * end_curvature;
    let b = start_tangent.cross(end_tangent);
    let delta = end - start;
    let c1 = -delta.y * start_tangent.x + delta.x * start_tangent.y;
    let c2 = delta.y * end_tangent.x - delta.x * end_tangent.y;
    let lambda0 = -c2 / b - a2 * c1 * c1 / (b * b * b);
    let lambda3 = -c1 / b;
    CubicBezier {
        p0: start,
        p1: start
            + Point::new(
                (lambda0 * start_tangent.x).max(0.0),
                (lambda0 * start_tangent.y).max(0.0),
            ),
        p2: end
            - Point::new(
                (lambda3 * end_tangent.x).max(0.0),
                (lambda3 * end_tangent.y).max(0.0),
            ),
        p3: end,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Point {
    x: f64,
    y: f64,
}

impl Point {
    const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    fn cross(self, other: Self) -> f64 {
        self.x * other.y - self.y * other.x
    }
}

impl std::ops::Add for Point {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        Self::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl std::ops::Sub for Point {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self::Output {
        Self::new(self.x - rhs.x, self.y - rhs.y)
    }
}

impl std::ops::Mul<f64> for Point {
    type Output = Self;
    fn mul(self, rhs: f64) -> Self::Output {
        Self::new(self.x * rhs, self.y * rhs)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct CubicBezier {
    p0: Point,
    p1: Point,
    p2: Point,
    p3: Point,
}

struct Builder {
    path: PathBuilder,
    origin_x: f64,
    origin_y: f64,
}

impl Builder {
    fn new(origin_x: f64, origin_y: f64) -> Self {
        Self {
            path: PathBuilder::new(),
            origin_x,
            origin_y,
        }
    }

    fn point(&self, x: f64, y: f64) -> SkPoint {
        SkPoint::new((self.origin_x + x) as f32, (self.origin_y + y) as f32)
    }

    fn move_to(&mut self, x: f64, y: f64) {
        let point = self.point(x, y);
        self.path.move_to(point);
    }

    fn line_to(&mut self, x: f64, y: f64) {
        let point = self.point(x, y);
        self.path.line_to(point);
    }

    #[allow(clippy::too_many_arguments)]
    fn cubic_to(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, x3: f64, y3: f64) {
        let p1 = self.point(x1, y1);
        let p2 = self.point(x2, y2);
        let p3 = self.point(x3, y3);
        self.path.cubic_to(p1, p2, p3);
    }

    fn arc_to_scaled(
        &mut self,
        center: Point,
        radius: f64,
        radius_scale: f64,
        start_angle: f64,
        sweep_angle: f64,
    ) {
        if sweep_angle.abs() <= f64::EPSILON {
            return;
        }
        let center_angle = start_angle + sweep_angle * 0.5;
        let center = center
            + Point::new(center_angle.cos(), center_angle.sin()) * radius * (1.0 - radius_scale);
        let radius = radius * radius_scale;
        let oval = Rect::new(
            (self.origin_x + center.x - radius) as f32,
            (self.origin_y + center.y - radius) as f32,
            (self.origin_x + center.x + radius) as f32,
            (self.origin_y + center.y + radius) as f32,
        );
        self.path.arc_to(
            oval,
            start_angle.to_degrees() as f32,
            sweep_angle.to_degrees() as f32,
            false,
        );
    }

    fn close(&mut self) {
        self.path.close();
    }

    fn finish(mut self) -> Path {
        self.path.detach()
    }
}

fn constrained_ratio(half_extent: f64, radius: f64, extended_fraction: f64) -> f64 {
    if radius <= 0.0 || extended_fraction <= 0.0 {
        return 1.0;
    }
    ((half_extent / radius - 1.0) / extended_fraction).clamp(0.0, 1.0)
}

fn finite_non_negative(value: f32) -> f64 {
    if value.is_finite() {
        value.max(0.0) as f64
    } else {
        0.0
    }
}

fn fit_ratio(available: f64, requested: f64) -> f64 {
    if requested > 0.0 {
        (available / requested).min(1.0)
    } else {
        1.0
    }
}

fn lerp(start: f64, stop: f64, fraction: f64) -> f64 {
    start + (stop - start) * fraction
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn continuous_rect_preserves_requested_bounds() {
        let rect = Rect::new(10.0, 20.0, 110.0, 120.0);
        let path = continuous_rounded_rect(rect, 18.0);
        let bounds = path.compute_tight_bounds();
        assert!((bounds.left - rect.left).abs() < 0.01);
        assert!((bounds.top - rect.top).abs() < 0.01);
        assert!((bounds.right - rect.right).abs() < 0.01);
        assert!((bounds.bottom - rect.bottom).abs() < 0.01);
        assert!(path.contains((60.0, 70.0)));
        assert!(!path.contains((10.0, 20.0)));
    }

    #[test]
    fn oversized_radii_are_scaled_without_leaving_the_rect() {
        let rect = Rect::new(0.0, 0.0, 80.0, 40.0);
        let path = continuous_rounded_rect(rect, 100.0);
        let bounds = path.compute_tight_bounds();
        assert!(bounds.left >= rect.left - 0.01);
        assert!(bounds.top >= rect.top - 0.01);
        assert!(bounds.right <= rect.right + 0.01);
        assert!(bounds.bottom <= rect.bottom + 0.01);
    }

    #[test]
    fn zero_radius_is_an_ordinary_rectangle() {
        let rect = Rect::new(3.0, 4.0, 23.0, 24.0);
        let path = continuous_rounded_rect(rect, 0.0);
        assert!(path.is_rect().is_some());
    }

    #[test]
    fn default_profile_has_matching_bezier_and_arc_curvature() {
        let profile = G2Profile::ROUNDED_RECTANGLE;
        assert_eq!(profile.bezier_curvature_scale, profile.arc_curvature_scale);
        let bezier = profile.bezier();
        for point in [bezier.p0, bezier.p1, bezier.p2, bezier.p3] {
            assert!(point.x.is_finite() && point.y.is_finite());
        }
    }
}
