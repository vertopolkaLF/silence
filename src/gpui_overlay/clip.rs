//! Convex polygon clipping for decorative geometry inside rounded card surfaces.
//! GPUI's content mask clips rectangles only; rounded corners need geometry clipping.
pub(super) type Vertex = (f32, f32);

pub(super) fn rounded_rect(width: f32, height: f32, radius: f32) -> Vec<Vertex> {
    let radius = radius.max(0.).min(width / 2.).min(height / 2.);
    if radius < 0.001 {
        return vec![(0., 0.), (width, 0.), (width, height), (0., height)];
    }
    let mut boundary = Vec::with_capacity(100);
    for (cx, cy, start) in [
        (radius, radius, std::f32::consts::PI),
        (width - radius, radius, std::f32::consts::PI * 1.5),
        (width - radius, height - radius, 0.),
        (radius, height - radius, std::f32::consts::FRAC_PI_2),
    ] {
        for step in 0..=24 {
            let angle = start + step as f32 / 24. * std::f32::consts::FRAC_PI_2;
            boundary.push((cx + radius * angle.cos(), cy + radius * angle.sin()));
        }
    }
    // At full pill radius adjacent quarter-arcs share an endpoint. Floating
    // trig error can turn that duplicate into a reversed microscopic edge,
    // whose half-plane wrongly clips away the entire decoration.
    boundary.dedup_by(|a, b| (a.0 - b.0).powi(2) + (a.1 - b.1).powi(2) < 0.00000001);
    if let (Some(first), Some(last)) = (boundary.first(), boundary.last()) {
        if (first.0 - last.0).powi(2) + (first.1 - last.1).powi(2) < 0.00000001 {
            boundary.pop();
        }
    }
    boundary
}

fn distance(a: Vertex, b: Vertex, p: Vertex) -> f32 {
    (b.0 - a.0) * (p.1 - a.1) - (b.1 - a.1) * (p.0 - a.0)
}

pub(super) fn polygon(mut subject: Vec<Vertex>, boundary: &[Vertex]) -> Vec<Vertex> {
    let mut output = Vec::with_capacity(subject.len() + boundary.len());
    for index in 0..boundary.len() {
        if subject.is_empty() {
            break;
        }
        output.clear();
        let a = boundary[index];
        let b = boundary[(index + 1) % boundary.len()];
        if (b.0 - a.0).powi(2) + (b.1 - a.1).powi(2) < 0.00000001 {
            continue;
        }
        let mut previous = *subject.last().unwrap();
        let mut previous_distance = distance(a, b, previous);
        for &current in &subject {
            let current_distance = distance(a, b, current);
            if (previous_distance >= 0.) != (current_distance >= 0.) {
                let t = previous_distance / (previous_distance - current_distance);
                output.push((
                    previous.0 + t * (current.0 - previous.0),
                    previous.1 + t * (current.1 - previous.1),
                ));
            }
            if current_distance >= 0. {
                output.push(current);
            }
            previous = current;
            previous_distance = current_distance;
        }
        std::mem::swap(&mut subject, &mut output);
    }
    subject
}

pub(super) fn circle(cx: f32, cy: f32, radius: f32) -> Vec<Vertex> {
    (0..192)
        .map(|step| {
            let angle = step as f32 / 192. * std::f32::consts::TAU;
            (cx + radius * angle.cos(), cy + radius * angle.sin())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn within_round_rect((x, y): Vertex, w: f32, h: f32, r: f32) -> bool {
        let r = r.min(w / 2.).min(h / 2.);
        let cx = x.clamp(r, w - r);
        let cy = y.clamp(r, h - r);
        x >= -0.001
            && y >= -0.001
            && x <= w + 0.001
            && y <= h + 0.001
            && (x - cx).powi(2) + (y - cy).powi(2) <= r * r + 0.01
    }

    #[test]
    fn animated_radar_never_leaks_past_rounded_corners() {
        for (w, h) in [(54., 54.), (180., 54.)] {
            for radius in [0., 8., 24., 32.] {
                let boundary = rounded_rect(w, h, radius);
                for step in 0..120 {
                    let ring_radius = (22. + step as f32 / 120. * 64.) / 2.;
                    for thickness in [0., -1.] {
                        let clipped =
                            polygon(circle(23., h / 2., ring_radius + thickness), &boundary);
                        assert!(clipped.iter().all(|&p| within_round_rect(p, w, h, radius)));
                    }
                }
            }
        }
    }

    #[test]
    fn radar_remains_visible_at_full_pill_radius() {
        for (w, h, cx) in [(54., 54., 27.), (180., 54., 23.)] {
            for radius in [26., 27., 28., 32.] {
                let boundary = rounded_rect(w, h, radius);
                let clipped = polygon(circle(cx, h / 2., 12.), &boundary);
                assert!(
                    clipped.len() >= 3,
                    "visible ring vanished: width={w}, radius={radius}"
                );
                let area = clipped
                    .iter()
                    .zip(clipped.iter().cycle().skip(1))
                    .map(|(a, b)| a.0 * b.1 - b.0 * a.1)
                    .sum::<f32>()
                    .abs()
                    / 2.;
                assert!(
                    area > 400.,
                    "interior circle was incorrectly cut away: {area}"
                );
            }
        }
    }

    #[test]
    fn paper_and_terminal_lines_remain_visible_at_full_pill_radius() {
        for h in [42., 54.] {
            for radius in 0..=32 {
                let boundary = rounded_rect(180., h, radius as f32);
                for y in [h / 2. - 12., h / 2. + 12.] {
                    let clipped = polygon(
                        vec![(0., y), (180., y), (180., y + 1.), (0., y + 1.)],
                        &boundary,
                    );
                    assert!(clipped.len() >= 3, "ruled line vanished: h={h}, r={radius}");
                    let area = clipped
                        .iter()
                        .zip(clipped.iter().cycle().skip(1))
                        .map(|(a, b)| a.0 * b.1 - b.0 * a.1)
                        .sum::<f32>()
                        .abs()
                        / 2.;
                    assert!(
                        area > 100.,
                        "ruled line lost its interior: h={h}, r={radius}, area={area}"
                    );
                }
            }
        }
    }

    #[test]
    fn moving_terminal_lines_are_trimmed_at_every_radius() {
        for radius in [0., 3., 21., 32.] {
            let boundary = rounded_rect(160., 42., radius);
            for step in 0..80 {
                let offset = step as f32 / 80. * 4.;
                for row in -1..13 {
                    let y = 3. + row as f32 * 4. - offset;
                    let clipped = polygon(
                        vec![(0., y), (160., y), (160., y + 1.), (0., y + 1.)],
                        &boundary,
                    );
                    assert!(
                        clipped
                            .iter()
                            .all(|&p| within_round_rect(p, 160., 42., radius))
                    );
                    if y > 42. || y + 1. < 0. {
                        assert!(clipped.is_empty());
                    }
                }
            }
        }
    }

    #[test]
    fn square_and_pill_contours_keep_the_expected_bounds() {
        let square = rounded_rect(80., 40., 0.);
        assert_eq!(
            polygon(
                vec![(-10., -10.), (90., -10.), (90., 50.), (-10., 50.)],
                &square
            )
            .len(),
            4
        );
        let pill = rounded_rect(40., 40., 32.);
        assert!(pill.iter().all(|&p| within_round_rect(p, 40., 40., 32.)));
    }
}
