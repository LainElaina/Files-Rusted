use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct DragPoint {
    pub(super) x: f32,
    pub(super) y: f32,
}

impl DragPoint {
    pub(super) fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct DragRect {
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) width: f32,
    pub(super) height: f32,
}

impl DragRect {
    pub(super) fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub(super) fn from_points(start: DragPoint, end: DragPoint) -> Self {
        let left = start.x.min(end.x);
        let top = start.y.min(end.y);
        let right = start.x.max(end.x);
        let bottom = start.y.max(end.y);
        Self::new(left, top, right - left, bottom - top)
    }

    pub(super) fn intersects(self, other: Self) -> bool {
        let self_right = self.x + self.width;
        let self_bottom = self.y + self.height;
        let other_right = other.x + other.width;
        let other_bottom = other.y + other.height;

        self.x < other_right
            && self_right > other.x
            && self.y < other_bottom
            && self_bottom > other.y
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct DragScrollViewport {
    pub(super) content_top: f32,
    pub(super) content_height: f32,
    pub(super) hot_zone_size: f32,
    pub(super) max_speed: f32,
}

pub(super) fn compute_drag_autoscroll_delta(pointer_y: f32, viewport: DragScrollViewport) -> f32 {
    if viewport.content_height <= 0.0 || viewport.hot_zone_size <= 0.0 || viewport.max_speed <= 0.0 {
        return 0.0;
    }

    let content_bottom = viewport.content_top + viewport.content_height;
    let hot_zone = viewport
        .hot_zone_size
        .min(viewport.content_height / 2.0)
        .max(0.0);

    if hot_zone == 0.0 {
        return 0.0;
    }

    let top_zone_end = viewport.content_top + hot_zone;
    if pointer_y < top_zone_end {
        let intensity = ((top_zone_end - pointer_y) / hot_zone).clamp(0.0, 1.0);
        return -viewport.max_speed * intensity;
    }

    let bottom_zone_start = content_bottom - hot_zone;
    if pointer_y > bottom_zone_start {
        let intensity = ((pointer_y - bottom_zone_start) / hot_zone).clamp(0.0, 1.0);
        return viewport.max_speed * intensity;
    }

    0.0
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct VisibleItemLayout {
    pub(super) path: PathBuf,
    pub(super) rect: DragRect,
}

#[derive(Clone, Debug)]
pub(super) struct DragSelectionSnapshot {
    pub(super) selected: Vec<PathBuf>,
    pub(super) primary: Option<PathBuf>,
    pub(super) anchor: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct DragSelectionResult {
    pub(super) selected: Vec<PathBuf>,
    pub(super) primary: Option<PathBuf>,
    pub(super) anchor: Option<PathBuf>,
    pub(super) rect: Option<DragRect>,
}

#[derive(Clone, Debug)]
pub(super) struct DragSelectionSession {
    start: DragPoint,
    pub(super) control: bool,
    pub(super) baseline: DragSelectionSnapshot,
}

impl DragSelectionSession {
    pub(super) fn begin(start: DragPoint, control: bool, baseline: DragSelectionSnapshot) -> Self {
        Self {
            start,
            control,
            baseline,
        }
    }

    pub(super) fn selection_for(
        &self,
        current: DragPoint,
        layouts: &[VisibleItemLayout],
        threshold: f32,
    ) -> DragSelectionResult {
        if drag_distance(self.start, current) < threshold {
            return DragSelectionResult {
                selected: self.baseline.selected.clone(),
                primary: self.baseline.primary.clone(),
                anchor: self.baseline.anchor.clone(),
                rect: None,
            };
        }

        let rect = DragRect::from_points(self.start, current);
        let hit_paths = layouts
            .iter()
            .filter(|layout| rect.intersects(layout.rect))
            .map(|layout| layout.path.clone())
            .collect::<Vec<_>>();

        let selected = if self.control {
            toggle_drag_selection(&self.baseline.selected, &hit_paths, layouts)
        } else {
            hit_paths.clone()
        };
        let primary = hit_paths.last().cloned();
        let anchor = primary.clone();

        DragSelectionResult {
            selected,
            primary,
            anchor,
            rect: Some(rect),
        }
    }
}

pub(super) fn drag_distance(start: DragPoint, end: DragPoint) -> f32 {
    let delta_x = end.x - start.x;
    let delta_y = end.y - start.y;
    (delta_x * delta_x + delta_y * delta_y).sqrt()
}

pub(super) fn toggle_drag_selection(
    baseline: &[PathBuf],
    hits: &[PathBuf],
    layouts: &[VisibleItemLayout],
) -> Vec<PathBuf> {
    let mut toggled = baseline.to_vec();
    for hit in hits {
        if let Some(index) = toggled.iter().position(|path| path == hit) {
            toggled.remove(index);
        } else {
            toggled.push(hit.clone());
        }
    }

    let mut ordered = layouts
        .iter()
        .filter_map(|layout| {
            toggled
                .iter()
                .any(|path| path == &layout.path)
                .then(|| layout.path.clone())
        })
        .collect::<Vec<_>>();

    for path in toggled {
        if ordered.iter().any(|existing| existing == &path) {
            continue;
        }
        ordered.push(path);
    }

    ordered
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn autoscroll_has_exact_top_and_bottom_boundaries() {
        let viewport = DragScrollViewport {
            content_top: 100.0,
            content_height: 400.0,
            hot_zone_size: 32.0,
            max_speed: 24.0,
        };

        assert_eq!(compute_drag_autoscroll_delta(132.0, viewport), 0.0);
        assert_eq!(compute_drag_autoscroll_delta(468.0, viewport), 0.0);
        assert_eq!(compute_drag_autoscroll_delta(100.0, viewport), -24.0);
        assert_eq!(compute_drag_autoscroll_delta(500.0, viewport), 24.0);
    }

    #[test]
    fn autoscroll_clamps_to_max_speed_beyond_viewport_edges() {
        let viewport = DragScrollViewport {
            content_top: 100.0,
            content_height: 400.0,
            hot_zone_size: 32.0,
            max_speed: 24.0,
        };

        assert_eq!(compute_drag_autoscroll_delta(-200.0, viewport), -24.0);
        assert_eq!(compute_drag_autoscroll_delta(900.0, viewport), 24.0);
    }

    #[test]
    fn autoscroll_returns_zero_for_invalid_viewports() {
        assert_eq!(
            compute_drag_autoscroll_delta(
                120.0,
                DragScrollViewport {
                    content_top: 100.0,
                    content_height: 0.0,
                    hot_zone_size: 32.0,
                    max_speed: 24.0,
                },
            ),
            0.0
        );
        assert_eq!(
            compute_drag_autoscroll_delta(
                120.0,
                DragScrollViewport {
                    content_top: 100.0,
                    content_height: -1.0,
                    hot_zone_size: 32.0,
                    max_speed: 24.0,
                },
            ),
            0.0
        );
        assert_eq!(
            compute_drag_autoscroll_delta(
                120.0,
                DragScrollViewport {
                    content_top: 100.0,
                    content_height: 400.0,
                    hot_zone_size: 0.0,
                    max_speed: 24.0,
                },
            ),
            0.0
        );
        assert_eq!(
            compute_drag_autoscroll_delta(
                120.0,
                DragScrollViewport {
                    content_top: 100.0,
                    content_height: 400.0,
                    hot_zone_size: 32.0,
                    max_speed: 0.0,
                },
            ),
            0.0
        );
    }

    #[test]
    fn autoscroll_clamps_hot_zone_to_half_height() {
        let viewport = DragScrollViewport {
            content_top: 0.0,
            content_height: 100.0,
            hot_zone_size: 80.0,
            max_speed: 24.0,
        };

        assert_eq!(compute_drag_autoscroll_delta(50.0, viewport), 0.0);
        assert_eq!(compute_drag_autoscroll_delta(25.0, viewport), -12.0);
        assert_eq!(compute_drag_autoscroll_delta(75.0, viewport), 12.0);
        assert_eq!(compute_drag_autoscroll_delta(-20.0, viewport), -24.0);
        assert_eq!(compute_drag_autoscroll_delta(120.0, viewport), 24.0);
    }
}
